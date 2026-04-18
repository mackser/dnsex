use crate::error::DnsexError;
use crate::server::Server;
use async_trait::async_trait;
use data_encoding::BASE32_NOPAD;
use hickory_proto::op::{Header, ResponseCode};
use hickory_proto::rr::{RData, Record, RecordType, rdata::TXT};
use hickory_server::{
    authority::MessageResponseBuilder,
    server::{Request, RequestHandler, ResponseHandler, ResponseInfo},
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct Transfer {
    filename: String,
    total_chunks: usize,
    chunks: HashMap<usize, Vec<u8>>,
}

impl Transfer {
    pub fn verify(&self) -> bool {
        self.chunks.len() == self.total_chunks
    }

    pub fn missing(&self) -> Vec<usize> {
        return (0..self.total_chunks).filter(|seq| !self.chunks.contains_key(seq)).collect();
    }
}

#[repr(u32)]
#[derive(Clone, Debug, Copy)]
pub enum ChunkFlag {
    Init = 1 << 0,
    Data = 1 << 1,
    Fin = 1 << 2,
}

#[derive(Clone, Debug)]
pub struct Chunk {
    pub data: Vec<u8>,
    pub seq: usize,
    pub id: String,
    pub flags: u32,
}

impl Chunk {
    pub fn has_flag(&self, flag: ChunkFlag) -> bool {
        (self.flags & (flag as u32)) != 0
    }
}

#[derive(Clone, Debug)]
pub struct DnsHandler {
    pub server: Arc<Server>,
    pub transfers: Arc<Mutex<HashMap<String, Transfer>>>,
}

impl DnsHandler {
    fn extract_chunk(&self, qname: &str) -> Option<Chunk> {
        let qname = qname.to_lowercase();
        let mut base_domain = self.server.config.domain.to_lowercase();
        if !base_domain.ends_with('.') {
            base_domain.push('.');
        }

        let remainder = qname.strip_suffix(&base_domain)?.trim_end_matches('.');
        let mut parts = remainder.split('.');
        let hex_data = parts.next()?;
        let seq_str = parts.next()?;
        let id = parts.next()?;
        let flags_str = parts.next()?;

        if parts.next().is_some() {
            return None;
        }

        let seq = seq_str.parse::<usize>().ok()?;
        let data = BASE32_NOPAD.decode(hex_data.to_uppercase().as_bytes()).ok()?;
        let flags = flags_str.parse::<u32>().ok()?;

        Some(Chunk {
            seq,
            data,
            id: id.to_string(),
            flags,
        })
    }

    async fn remove_transfer(&self, chunk_id: &str) -> Result<Transfer, DnsexError> {
        let mut active_transfers = self.transfers.lock().await;
        let transfer = active_transfers
            .remove(chunk_id)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Transfer not found"))?;

        drop(active_transfers);
        Ok(transfer)
    }

    async fn save_transfer(&self, chunk: &Chunk) -> Result<(), DnsexError> {
        fs::create_dir_all(&self.server.config.output).await?;
        let mut transfer = self.remove_transfer(&chunk.id).await?;
        let mut sequences: Vec<usize> = transfer.chunks.keys().copied().collect();
        sequences.sort();

        let mut final_data = Vec::new();
        for seq in sequences {
            if let Some(mut chunk_data) = transfer.chunks.remove(&seq) {
                final_data.append(&mut chunk_data);
            }
        }

        let joined_path = Path::new(&self.server.config.output).join(&transfer.filename);
        if let Some(parent) = joined_path.parent() {
            let _ = fs::create_dir_all(parent).await?;
        }

        let mut file = fs::OpenOptions::new().write(true).create(true).open(&joined_path).await?;

        file.write_all(&final_data).await?;
        println!("{}: Fin (Saved {} bytes)", chunk.id, final_data.len());

        Ok(())
    }

    async fn respond_refused(
        &self,
        mut response_handle: impl ResponseHandler,
        builder: MessageResponseBuilder<'_>,
        mut header: Header,
    ) -> ResponseInfo {
        header.set_response_code(ResponseCode::Refused);
        let response = builder.build_no_records(header);

        match response_handle.send_response(response).await {
            Ok(info) => return info,
            Err(e) => {
                eprintln!("Failed to send DNS response: {}", e);

                let mut header = Header::new();
                header.set_response_code(ResponseCode::ServFail);
                return header.into();
            }
        };
    }
}

#[async_trait]
impl RequestHandler for DnsHandler {
    async fn handle_request<R: ResponseHandler>(&self, request: &Request, mut response_handle: R) -> ResponseInfo {
        let query = request.query();
        let qname = query.name();
        let qname_str = qname.to_string().to_lowercase();
        let record_type = query.query_type();

        let builder = MessageResponseBuilder::from_message_request(request);
        let mut header = Header::response_from_request(request.header());

        let expected_suffix = format!("{}.", self.server.config.domain);
        if !qname_str.ends_with(&expected_suffix) {
            return self.respond_refused(response_handle, builder, header).await;
        }

        if record_type == RecordType::TXT {
            if let Some(chunk) = self.extract_chunk(&qname_str) {
                if chunk.has_flag(ChunkFlag::Init) {
                    let filename = String::from_utf8_lossy(&chunk.data);
                    let mut active_transfers = self.transfers.lock().await;
                    if let Some(transfer) = active_transfers.get_mut(&chunk.id) {
                        transfer.filename.push_str(&filename);
                    } else {
                        let transfer = Transfer {
                            filename: filename.to_string(),
                            total_chunks: chunk.seq,
                            chunks: HashMap::new(),
                        };

                        active_transfers.insert(chunk.id.clone(), transfer);
                    }

                    let rdata = RData::TXT(TXT::new(vec!["OK".into()]));
                    let record = Record::from_rdata(qname.into(), 60, rdata);
                    header.set_response_code(ResponseCode::NoError);

                    let response = builder.build(
                        header,
                        vec![&record].into_iter(),
                        vec![].into_iter(),
                        vec![].into_iter(),
                        vec![].into_iter(),
                    );

                    return response_handle.send_response(response).await.unwrap();
                } else if chunk.has_flag(ChunkFlag::Data) {
                    let mut active_transfers = self.transfers.lock().await;
                    if let Some(transfer) = active_transfers.get_mut(&chunk.id) {
                        transfer.chunks.insert(chunk.seq, chunk.data);
                        println!("{}: Data (Seq {})", chunk.id, chunk.seq);
                    }
                } else if chunk.has_flag(ChunkFlag::Fin) {
                    let (response_text, should_save) = {
                        let active_transfers = self.transfers.lock().await;
                        if let Some(transfer) = active_transfers.get(&chunk.id) {
                            if !transfer.verify() {
                                let missing_str = transfer.missing().iter().map(|m| m.to_string()).collect::<Vec<_>>().join(",");

                                (format!("MISSING:{}", missing_str), false)
                            } else {
                                (String::from("OK"), true)
                            }
                        } else {
                            (String::from("ERROR: Not Found"), false)
                        }
                    };

                    if should_save {
                        if let Err(e) = self.save_transfer(&chunk).await {
                            eprintln!("UNEXPECTED: Failed to save {}: {}", chunk.id, e);
                            header.set_response_code(ResponseCode::ServFail);
                            let response = builder.build_no_records(header);

                            match response_handle.send_response(response).await {
                                Ok(info) => return info,
                                Err(err) => {
                                    eprintln!("Failed to send ServFail: {}", err);
                                    return ResponseInfo::from(hickory_proto::op::Header::new());
                                }
                            }
                        }
                    }

                    let rdata = RData::TXT(TXT::new(vec![response_text]));
                    let record = Record::from_rdata(qname.into(), 60, rdata);
                    header.set_response_code(ResponseCode::NoError);

                    let response = builder.build(
                        header,
                        vec![&record].into_iter(),
                        vec![].into_iter(),
                        vec![].into_iter(),
                        vec![].into_iter(),
                    );

                    return response_handle.send_response(response).await.unwrap();
                }

                let txt: Vec<String> = vec![chunk.seq.to_string()];
                let rdata = RData::TXT(TXT::new(txt));
                let record = Record::from_rdata(qname.into(), 60, rdata);
                header.set_response_code(ResponseCode::NoError);

                let response = builder.build(
                    header,
                    vec![&record].into_iter(),
                    vec![].into_iter(),
                    vec![].into_iter(),
                    vec![].into_iter(),
                );

                return response_handle.send_response(response).await.unwrap();
            }
        }

        header.set_response_code(ResponseCode::NXDomain);
        let response = builder.build_no_records(header);
        return response_handle.send_response(response).await.unwrap();
    }
}

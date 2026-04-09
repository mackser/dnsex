use async_trait::async_trait;
use hickory_proto::op::ResponseCode;
use hickory_proto::rr::{RData, Record, RecordType, rdata::TXT};
use hickory_server::{
    authority::MessageResponseBuilder,
    server::{Request, RequestHandler, ResponseHandler, ResponseInfo},
};
use std::collections::HashMap;

use crate::error::DnsexError;
use crate::server::Server;
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

#[derive(Clone, Debug)]
pub enum ChunkFlag {
    Init,
    Data,
    Fin,
}

#[derive(Clone, Debug)]
pub struct Chunk {
    pub data: Vec<u8>,
    pub seq: usize,
    pub id: String,
    pub flag: ChunkFlag,
}

#[derive(Clone, Debug)]
pub struct DnsHandler {
    pub server: Arc<Server>,
    pub transfers: Arc<Mutex<HashMap<String, Transfer>>>,
}

impl DnsHandler {
    fn extract_chunk(&self, qname: &str) -> Option<Chunk> {
        let qname = qname.to_lowercase();
        let mut base_domain = self.server.domain.to_lowercase();
        if !base_domain.ends_with('.') {
            base_domain.push('.');
        }

        let remainder = qname.strip_suffix(&base_domain)?.trim_end_matches('.');
        let mut parts = remainder.split('.');
        let hex_data = parts.next()?;
        let seq_str = parts.next()?;
        let id = parts.next()?;
        let flag_str = parts.next()?;

        if parts.next().is_some() {
            return None;
        }

        let seq = seq_str.parse::<usize>().ok()?;
        let data = hex::decode(hex_data).ok()?;
        let flag = match flag_str {
            "i" => ChunkFlag::Init,
            "d" => ChunkFlag::Data,
            "f" => ChunkFlag::Fin,
            _ => return None,
        };

        Some(Chunk {
            seq,
            data,
            id: id.to_string(),
            flag,
        })
    }
}

#[async_trait]
impl RequestHandler for DnsHandler {
    async fn handle_request<R: ResponseHandler>(
        &self,
        request: &Request,
        mut response_handle: R,
    ) -> ResponseInfo {
        let query = request.query();
        let qname = query.name();
        let qname_str = qname.to_string().to_lowercase();
        let record_type = query.query_type();

        let builder = MessageResponseBuilder::from_message_request(request);
        let mut header = hickory_proto::op::Header::response_from_request(request.header());

        let expected_suffix = format!("{}.", self.server.domain);
        if !qname_str.ends_with(&expected_suffix) {
            header.set_response_code(ResponseCode::Refused);
            let response = builder.build_no_records(header);

            match response_handle.send_response(response).await {
                Ok(info) => return info,
                Err(e) => {
                    eprintln!("Failed to send DNS response: {}", e);

                    let mut header = hickory_proto::op::Header::new();
                    header.set_response_code(ResponseCode::ServFail);
                    return header.into();
                }
            };
        }

        if record_type == RecordType::TXT {
            if let Some(chunk) = self.extract_chunk(&qname_str) {
                match chunk.flag {
                    ChunkFlag::Init => {
                        let filename = String::from_utf8_lossy(&chunk.data);
                        let transfer = Transfer {
                            filename: filename.to_string(),
                            total_chunks: chunk.seq,
                            chunks: HashMap::new(),
                        };

                        let mut active_transfers = self.transfers.lock().await;
                        active_transfers.insert(chunk.id.clone(), transfer);
                        println!("{}: Init", filename);
                    }
                    ChunkFlag::Data => {
                        let mut active_transfers = self.transfers.lock().await;
                        if let Some(transfer) = active_transfers.get_mut(&chunk.id) {
                            transfer.chunks.insert(chunk.seq, chunk.data);
                            println!("{}: Data (Seq {})", chunk.id, chunk.seq);
                        }
                    }
                    ChunkFlag::Fin => {
                        let mut active_transfers = self.transfers.lock().await;
                        if let Some(mut transfer) = active_transfers.remove(&chunk.id) {
                            drop(active_transfers);

                            let mut sequences: Vec<usize> =
                                transfer.chunks.keys().copied().collect();
                            sequences.sort();

                            let mut final_data = Vec::new();
                            for seq in sequences {
                                if let Some(mut chunk_data) = transfer.chunks.remove(&seq) {
                                    final_data.append(&mut chunk_data);
                                }
                            }

                            let mut file = fs::OpenOptions::new()
                                .write(true)
                                .create(true)
                                .open(&transfer.filename)
                                .await
                                .unwrap();

                            file.write_all(&final_data).await.unwrap();
                            println!("{}: Fin (Saved {} bytes)", chunk.id, final_data.len());
                        }
                    }
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

use crate::error::DnsexError;
use crate::server::Server;
use async_trait::async_trait;
use data_encoding::BASE32_NOPAD;
use hickory_proto::op::{Header, ResponseCode};
use hickory_proto::rr::{Name, RData, Record, RecordType, rdata::TXT};
use hickory_server::{
    authority::MessageResponseBuilder,
    server::{Request, RequestHandler, ResponseHandler, ResponseInfo},
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::fs::{self, File};
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::Mutex;

#[derive(Debug)]
pub struct Transfer {
    filename: String,
    bufwriter: BufWriter<File>,
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

    async fn create_bufwriter(&self, filename: &str) -> Result<BufWriter<File>, DnsexError> {
        let joined_path = Path::new(&self.server.config.output).join(&filename);
        if let Some(parent) = joined_path.parent() {
            let _ = fs::create_dir_all(parent).await?;
        }

        let file = fs::OpenOptions::new().write(true).create(true).open(&joined_path).await?;
        let bufwriter = BufWriter::new(file);

        Ok(bufwriter)
    }

    async fn respond_error(
        &self,
        mut response_handle: impl ResponseHandler,
        builder: MessageResponseBuilder<'_>,
        mut header: Header,
        rcode: ResponseCode,
    ) -> ResponseInfo {
        header.set_response_code(rcode);
        let response = builder.build_no_records(header);

        match response_handle.send_response(response).await {
            Ok(info) => return info,
            Err(e) => {
                eprintln!("Failed to send DNS response: {}", e);
                let mut fallback = Header::new();
                fallback.set_response_code(ResponseCode::ServFail);
                fallback.into()
            }
        }
    }

    async fn respond_txt(
        &self,
        mut response_handle: impl ResponseHandler,
        builder: MessageResponseBuilder<'_>,
        mut header: Header,
        qname: Name,
        txt_data: Vec<String>,
    ) -> ResponseInfo {
        header.set_response_code(ResponseCode::NoError);
        let rdata = RData::TXT(TXT::new(txt_data));
        let record = Record::from_rdata(qname, 60, rdata);

        let response = builder.build(
            header,
            vec![&record].into_iter(),
            vec![].into_iter(),
            vec![].into_iter(),
            vec![].into_iter(),
        );

        match response_handle.send_response(response).await {
            Ok(info) => info,
            Err(e) => {
                eprintln!("Failed to send TXT response: {}", e);
                let mut fallback = Header::new();
                fallback.set_response_code(ResponseCode::ServFail);
                fallback.into()
            }
        }
    }
}

#[async_trait]
impl RequestHandler for DnsHandler {
    async fn handle_request<R: ResponseHandler>(&self, request: &Request, response_handle: R) -> ResponseInfo {
        let query = request.query();
        let qname = query.name();
        let qname_str = qname.to_string().to_lowercase();
        let record_type = query.query_type();

        let builder = MessageResponseBuilder::from_message_request(request);
        let header = Header::response_from_request(request.header());

        let expected_suffix = format!("{}.", self.server.config.domain);
        if !qname_str.ends_with(&expected_suffix) {
            return self.respond_error(response_handle, builder, header, ResponseCode::Refused).await;
        }

        if record_type == RecordType::TXT {
            if let Some(chunk) = self.extract_chunk(&qname_str) {
                if chunk.has_flag(ChunkFlag::Init) {
                    let filename = String::from_utf8_lossy(&chunk.data);
                    let mut active_transfers = self.transfers.lock().await;
                    if let Some(transfer) = active_transfers.get_mut(&chunk.id) {
                        transfer.filename.push_str(&filename);
                    } else {
                        let bufwriter = match self.create_bufwriter(&filename).await {
                            Ok(b) => b,
                            _ => return self.respond_error(response_handle, builder, header, ResponseCode::ServFail).await,
                        };

                        let transfer = Transfer {
                            filename: filename.to_string(),
                            bufwriter,
                        };

                        active_transfers.insert(chunk.id.clone(), transfer);
                    }

                    return self.respond_txt(response_handle, builder, header, qname.into(), vec!["OK".into()]).await;
                } else if chunk.has_flag(ChunkFlag::Data) {
                    let mut active_transfers = self.transfers.lock().await;
                    if let Some(transfer) = active_transfers.get_mut(&chunk.id) {
                        let _ = transfer.bufwriter.write_all(&chunk.data).await;
                    }
                } else if chunk.has_flag(ChunkFlag::Fin) {
                    let mut active_transfers = self.transfers.lock().await;
                    let response_text = if let Some(transfer) = active_transfers.get_mut(&chunk.id) {
                        let _ = transfer.bufwriter.flush().await;
                        String::from("OK")
                    } else {
                        String::from("ERROR: Not Found")
                    };

                    return self
                        .respond_txt(response_handle, builder, header, qname.into(), vec![response_text])
                        .await;
                }

                return self
                    .respond_txt(response_handle, builder, header, qname.into(), vec![chunk.seq.to_string()])
                    .await;
            }
        }

        return self.respond_error(response_handle, builder, header, ResponseCode::NXDomain).await;
    }
}

use async_trait::async_trait;
use hickory_proto::op::ResponseCode;
use hickory_proto::rr::{RData, Record, RecordType, rdata::A, rdata::TXT};
use hickory_server::{
    authority::MessageResponseBuilder,
    server::{Request, RequestHandler, ResponseHandler, ResponseInfo},
};

use crate::server::Server;
use std::io::Write;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Chunk {
    pub data: Vec<u8>,
    pub seq: usize,
}

#[derive(Clone, Debug)]
pub struct DnsHandler {
    pub server: Arc<Server>,
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

        if parts.next().is_some() {
            return None;
        }

        let seq = seq_str.parse::<usize>().ok()?;
        let data = hex::decode(hex_data).ok()?;

        Some(Chunk { seq, data })
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
        let qname_str = qname.to_string();
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

        if let Some(chunk) = self.extract_chunk(&qname_str) {
            let data = String::from_utf8_lossy(&chunk.data);
            print!("{}", data);
            let _ = std::io::stdout().flush();

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

        header.set_response_code(ResponseCode::NXDomain);
        let response = builder.build_no_records(header);
        return response_handle.send_response(response).await.unwrap();
    }
}

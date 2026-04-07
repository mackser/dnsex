use async_trait::async_trait;
use hickory_proto::op::ResponseCode;
use hickory_proto::rr::{RData, Record, RecordType, rdata::A};
use hickory_server::{
    authority::MessageResponseBuilder,
    server::{Request, RequestHandler, ResponseHandler, ResponseInfo},
};

use crate::server::Server;
use std::sync::Arc;
use std::io::Write;

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
    fn extract_chunk(&self, qname: &str) -> Chunk {
        let payload = self.extract_payload(qname);
        let parts: Vec<_> = payload.split('.').collect();
        let decoded_data = hex::decode(parts[0]).unwrap_or_default();
        let seq = parts[1].parse::<usize>().unwrap();

        Chunk { 
            data: decoded_data,
            seq: seq
        }
    }

    fn extract_payload<'a>(&self, qname: &'a str) -> &'a str {
        let domain: &str = self.server.domain.as_str();

        if let Some(idx) = qname.find(domain) {
            if idx == 0 || qname.as_bytes().get(idx.wrapping_sub(1)) == Some(&b'.') {
                let end = idx.checked_sub(1).unwrap_or(0);
                return &qname[..end];
            }
        }

        ""
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
        let qname = query.name().to_string();
        let record_type = query.query_type();

        let payload = self.extract_chunk(&qname);
        let data = String::from_utf8(payload.data).unwrap();
        print!("{}", data);
        let _ = std::io::stdout().flush();

        let builder = MessageResponseBuilder::from_message_request(request);
        let mut header = hickory_proto::op::Header::response_from_request(request.header());

        // if qname.to_string() == "test.local." && record_type == RecordType::A {
        //     let rdata = RData::A(A::new(1, 2, 3, 4));
        //     let record = Record::from_rdata(qname.into(), 60, rdata);

        //     let response = builder.build(
        //         header,
        //         vec![&record].into_iter(),
        //         vec![].into_iter(),
        //         vec![].into_iter(),
        //         vec![].into_iter(),
        //     );

        //     return response_handle.send_response(response).await.unwrap();
        // }

        header.set_response_code(ResponseCode::NXDomain);
        let response = builder.build_no_records(header);
        response_handle.send_response(response).await.unwrap()
    }
}

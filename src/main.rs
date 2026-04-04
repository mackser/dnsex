use async_trait::async_trait;
use hickory_proto::op::ResponseCode;
use hickory_proto::rr::{RData, Record, RecordType, rdata::A};
use hickory_server::{
    ServerFuture,
    authority::MessageResponseBuilder,
    server::{Request, RequestHandler, ResponseHandler, ResponseInfo},
};
use std::net::{Ipv4Addr, SocketAddr};
use tokio::net::{TcpListener, UdpSocket};

#[derive(Clone, Debug)]
struct DnsHandler;

#[async_trait]
impl RequestHandler for DnsHandler {
    async fn handle_request<R: ResponseHandler>(
        &self,
        request: &Request,
        mut response_handle: R,
    ) -> ResponseInfo {
        let query = request.query();
        let qname = query.name();
        let record_type = query.query_type();

        println!("Received query for: {} (Type: {})", qname, record_type);

        let builder = MessageResponseBuilder::from_message_request(request);
        let mut header = hickory_proto::op::Header::response_from_request(request.header());

        if qname.to_string() == "test.local." && record_type == RecordType::A {
            let rdata = RData::A(A::new(1, 2, 3, 4));
            let record = Record::from_rdata(qname.into(), 60, rdata);

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
        response_handle.send_response(response).await.unwrap()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let handler = DnsHandler;
    let mut server = ServerFuture::new(handler);

    let addr: SocketAddr = "0.0.0.0:8053".parse()?;

    let udp_socket = UdpSocket::bind(&addr).await?;
    let tcp_listener = TcpListener::bind(&addr).await?;

    println!("DNS server started on {}", addr);

    server.register_socket(udp_socket);
    server.register_listener(tcp_listener, std::time::Duration::from_secs(30));
    server.block_until_done().await?;

    Ok(())
}

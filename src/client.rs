use crate::error::DnsexError;

pub struct Client {
    domain: String,
}

impl Client {
    pub fn new(domain: impl Into<String>) -> Self {
        Self {
            domain: domain.into(),
        }
    }

    pub fn send(&self, data: String) -> Result<(), DnsexError> {




        Ok(())
    }
}

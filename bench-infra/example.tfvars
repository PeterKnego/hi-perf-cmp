# Copy to terraform.tfvars and fill in your values:
#     cp example.tfvars terraform.tfvars
# (Terraform only auto-loads terraform.tfvars.) Credentials are NOT here —
# set AWS_PROFILE or AWS_ACCESS_KEY_ID/SECRET in .env (see .env.example).
# Use a scoped IAM principal, NOT root account keys.

# Two nodes: node0 (client/driver + single-host benches) + node1 (RTT responder).
node_count = 2

# c6id.2xlarge = 8 vCPU, 1x 474 GB LOCAL NVMe instance store, sustained
# ~12.5 Gbps network. Satisfies BOTH the local-NVMe (filesystem-write) and
# cross-host-bandwidth (network-rtt) requirements. Do NOT use c7i (no NVMe).
instance_type = "c6id.2xlarge"

# Same-region single-AZ + cluster placement group (set by main.tf) keeps the
# inter-node private path low-latency.
region = "us-east-1"

ssh_public_key       = "ssh-ed25519 AAAA... you@host"  # AWS key pairs accept ed25519
ssh_private_key_file = "~/.ssh/id_ed25519"

allow_ssh_cidr = "203.0.113.4/32"  # your IP/32 — NOT 0.0.0.0/0

ttl_hours = 4          # advisory tag only — nothing auto-reaps; run `make destroy`
owner     = "hi-perf-cmp-bench"

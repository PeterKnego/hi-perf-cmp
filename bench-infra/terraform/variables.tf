variable "node_count" {
  description = "Number of bench hosts. Default 2: node0 (client/driver) + node1 (RTT responder)."
  type        = number
  default     = 2
}

variable "instance_type" {
  description = <<-EOT
    EC2 instance type. Default c6id.2xlarge: 8 vCPU, 1x 474 GB local NVMe
    instance store, sustained ~12.5 Gbps network — satisfies BOTH the local-NVMe
    (filesystem-write) and cross-host-bandwidth (network-rtt) requirements.
    Do NOT use c7i: it has no instance-store NVMe.
  EOT
  type        = string
  default     = "c6id.2xlarge"
}

variable "region" {
  description = "AWS region."
  type        = string
  default     = "us-east-1"
}

variable "ssh_public_key" {
  description = "SSH public key contents to install on the hosts (ed25519 accepted)."
  type        = string
}

variable "ssh_private_key_file" {
  description = "Path to the matching private key, parsed by the Makefile/inventory for Ansible."
  type        = string
}

variable "allow_ssh_cidr" {
  description = "CIDR allowed to SSH to the hosts (e.g. your IP/32). NOT 0.0.0.0/0."
  type        = string
}

variable "ttl_hours" {
  description = "Advisory TTL tag for the cost guard. Nothing auto-reaps; run `make destroy`."
  type        = number
  default     = 4
}

variable "owner" {
  description = "Owner tag/name prefix for resources."
  type        = string
  default     = "hi-perf-cmp-bench"
}

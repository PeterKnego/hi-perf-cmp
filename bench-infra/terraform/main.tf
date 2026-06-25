provider "aws" {
  region = var.region
  # Credentials come from the standard provider chain: env vars
  # (AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY/AWS_SESSION_TOKEN) or AWS_PROFILE,
  # exported by the Makefile from .env. No keys live in this repo.
}

data "aws_ami" "ubuntu" {
  most_recent = true
  owners      = ["099720109477"] # Canonical
  filter {
    name   = "name"
    values = ["ubuntu/images/hvm-ssd-gp3/ubuntu-noble-24.04-amd64-server-*"]
  }
}

resource "aws_vpc" "bench" {
  cidr_block           = "10.10.0.0/16"
  enable_dns_hostnames = true
  tags                 = { Name = "${var.owner}-vpc", owner = var.owner }
}

# Pick an AZ that actually offers the requested instance type. AWS otherwise
# auto-places the subnet in an arbitrary AZ where larger / NVMe types like
# c6id.2xlarge may not be offered -> RunInstances "Unsupported" 400.
data "aws_ec2_instance_type_offerings" "supported_az" {
  filter {
    name   = "instance-type"
    values = [var.instance_type]
  }
  location_type = "availability-zone"
}

resource "aws_subnet" "bench" {
  vpc_id                  = aws_vpc.bench.id
  cidr_block              = "10.10.1.0/24"
  map_public_ip_on_launch = true
  # Cluster placement group pins all nodes to this one AZ; pick a supported one.
  availability_zone = sort(data.aws_ec2_instance_type_offerings.supported_az.locations)[0]
  tags              = { Name = "${var.owner}-subnet", owner = var.owner }
}

resource "aws_internet_gateway" "bench" {
  vpc_id = aws_vpc.bench.id
  tags   = { Name = "${var.owner}-igw", owner = var.owner }
}

resource "aws_route_table" "bench" {
  vpc_id = aws_vpc.bench.id
  route {
    cidr_block = "0.0.0.0/0"
    gateway_id = aws_internet_gateway.bench.id
  }
  tags = { Name = "${var.owner}-rt", owner = var.owner }
}

resource "aws_route_table_association" "bench" {
  subnet_id      = aws_subnet.bench.id
  route_table_id = aws_route_table.bench.id
}

resource "aws_security_group" "bench" {
  name        = "${var.owner}-sg"
  description = "bench fleet: SSH from allow_ssh_cidr, all intra-SG, egress all"
  vpc_id      = aws_vpc.bench.id
  ingress {
    description = "ssh"
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = [var.allow_ssh_cidr]
  }
  ingress {
    # all intra-SG traffic (covers the RTT TCP/UDP ports between node0/node1)
    description = "intra-cluster"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    self        = true
  }
  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
  tags = { Name = "${var.owner}-sg", owner = var.owner }
}

resource "aws_key_pair" "bench" {
  key_name   = "${var.owner}-key"
  public_key = var.ssh_public_key
}

resource "aws_placement_group" "bench" {
  name     = "${var.owner}-pg"
  strategy = "cluster"
}

resource "aws_instance" "node" {
  count                  = var.node_count
  ami                    = data.aws_ami.ubuntu.id
  instance_type          = var.instance_type
  subnet_id              = aws_subnet.bench.id
  vpc_security_group_ids = [aws_security_group.bench.id]
  key_name               = aws_key_pair.bench.key_name
  placement_group        = aws_placement_group.bench.id
  private_ip             = "10.10.1.${count.index + 10}"
  tags = {
    Name      = "${var.owner}-node${count.index}"
    owner     = var.owner
    ttl_hours = tostring(var.ttl_hours)
    role      = "node${count.index}"
  }
}

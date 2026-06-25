output "nodes" {
  description = "Ordered [node0, node1, ...] with role + public/private IPs."
  value = [
    for i, s in aws_instance.node : {
      name       = s.tags["Name"]
      role       = "node${i}"
      public_ip  = s.public_ip
      private_ip = s.private_ip
    }
  ]
}

output "ssh_user" {
  description = "SSH username for Ansible (Ubuntu 24.04 default image user)."
  value       = "ubuntu"
}

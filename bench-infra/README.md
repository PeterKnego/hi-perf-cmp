# bench-infra — AWS provisioning rig for hi-perf-cmp

Spins up a small **AWS** fleet, installs the three toolchains (Rust / Go / Java),
builds the benchmarks from your local working tree, runs the benchmark matrix
across the focus areas, and pulls the result lines back to
`bench-out/dist/<ts>/results.jsonl`. AWS-only, Terraform + Ansible + Makefile +
`.env`. See the design at
`docs/superpowers/specs/2026-06-25-bench-infra-aws-design.md`.

> The paid run is **user-initiated**. `make up` / `make bench-oneshot` launch
> billable EC2 instances. Nothing auto-reaps — `make status` is the cost guard
> and `make destroy` is the teardown.

## Topology (two nodes)

Two EC2 instances in one VPC/subnet/AZ, in a cluster placement group for a
low-latency private path:

- **node0** — client/driver. Runs the `network-rtt` **client** and the
  single-host benchmarks (`filesystem-write`, `thread-handoff`). Holds the
  collected `results.jsonl`.
- **node1** — `network-rtt` **responder** (echo server) only.

`network-rtt` is measured cross-host over the private network (never loopback);
single-host benchmarks run on node0 only.

## NVMe requirement

The default `instance_type = c6id.2xlarge` has a **local NVMe instance store**.
The `os_tune` role detects it (model "Instance Storage"), `mkfs.ext4`-formats and
mounts it at `/opt/bench` **before** anything writes there, so the synced source
tree and the `filesystem-write` target land on real local NVMe — not
network-backed EBS. Do **not** switch to `c7i`: it has no instance-store NVMe.

## Control-machine prerequisites

These run on the machine that *drives* the rig (your laptop / a control box) —
**not** the provisioned hosts (those get the toolchains installed by Ansible).

| Tool | Min version | Used by |
|------|-------------|---------|
| `terraform` | >= 1.6 (tested 1.9.8) | `make init/up/destroy/status` |
| `ansible` (ansible-core) | >= 2.16 | `make up/bench` |
| collections `ansible.posix`, `community.general` | latest | `sysctl`, `synchronize` (rsync) |
| `jq` | any | inventory generator + `make status/ssh-node0` |
| `rsync` | any | source sync + result pull |
| an SSH keypair | — | host access (path goes in `terraform.tfvars`) |

Install without root (binaries into `~/.local/bin`, which must be on `PATH`):

```bash
mkdir -p ~/.local/bin
curl -fsSL -o /tmp/tf.zip https://releases.hashicorp.com/terraform/1.9.8/terraform_1.9.8_linux_amd64.zip
unzip -o /tmp/tf.zip terraform -d ~/.local/bin
python3 -m pip install --user --break-system-packages ansible-core
ansible-galaxy collection install ansible.posix community.general
sudo apt-get install -y jq rsync   # or brew on macOS
```

## Credentials

Put AWS credentials in a gitignored `bench-infra/.env` — the Makefile auto-loads
it and exports the vars into terraform/ansible, so no manual `export` is needed:

    cp .env.example .env   # then fill in

- `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` (+ `AWS_SESSION_TOKEN` for STS), or
- `AWS_PROFILE` (a named profile from `~/.aws/credentials`).

Use a **scoped IAM principal** (see `aws-iam-bench-policy.json`), not root keys.

> `.env` uses `KEY=value` (Make include syntax, not shell-sourced). Prefer bare
> values: Make keeps surrounding quotes literally, so `AWS_SECRET_ACCESS_KEY="x"`
> would reach terraform *with* quotes. The Makefile strips surrounding double
> quotes from the known cred vars as a safety net, but bare is cleanest.

## Quickstart

    cp example.tfvars terraform.tfvars   # edit ssh keys + allow_ssh_cidr
    make init
    make up            # tf apply -> inventory -> provision (toolchains + build)
    make bench         # run the matrix + collect to bench-out/dist/<ts>/
    make destroy       # tear down

One-shot: `make bench-oneshot` (up -> bench -> destroy). Persistent: `make up`
once, `make bench` repeatedly, `make ssh-node0` to investigate, `make destroy`
when done. `make status` lists hosts + uptime (cost guard).

## Results

The benchmarks emit one result-contract JSON line per metric to stdout (see
`docs/result-contract.md`). The `run` role appends every line to
`/opt/bench/results/results.jsonl` on node0; `collect` writes a `manifest.txt`
(timestamp, instance type, vCPUs, mem, kernel, RTT params, git SHA, node roles)
and pulls everything to:

    bench-out/dist/<ts>/results.jsonl
    bench-out/dist/<ts>/manifest.txt

## Benchmark matrix

`languages: [rust, go, java]` × `focus_areas` (in `ansible/group_vars/all.yml`):

- `network-rtt` (cross_host) — per language, sequentially: start the responder
  on node1, run the client on node0 (`RTT_HOST` = node1 private IP), then kill
  the responder. RTT params (`rtt_payload_bytes` / `rtt_warmup` /
  `rtt_iterations` / `rtt_tcp_port` / `rtt_udp_port`) come from `group_vars` and
  are identical across languages for a fair comparison.
- `filesystem-write`, `thread-handoff` (local) — run on node0 only;
  `filesystem-write` writes under the NVMe-backed `/opt/bench`.

Per-language invocation (in `roles/run/files/run_bench.sh`): Rust/Go exec the
prebuilt release binaries (`rust/target/release/<area>`, `go/bin/<area>`); Java
runs `./gradlew :<area>:run -q`.

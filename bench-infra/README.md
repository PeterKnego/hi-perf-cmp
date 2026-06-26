# bench-infra — AWS benchmark rig for hi-perf-cmp

Spins up a small **AWS** fleet, installs the three toolchains (Rust / Go / Java),
builds the benchmarks from your local working tree, runs the benchmark matrix
cross-host, and pulls the result lines back to `bench-out/dist/<ts>/`. AWS-only;
Terraform + Ansible + Makefile + a `.env` file. Design spec:
`docs/superpowers/specs/2026-06-25-bench-infra-aws-design.md`.

> ### ⚠️ This costs money and nothing cleans up automatically
> `make up` / `make bench-oneshot` launch billable EC2 instances (~**$0.81/hr**
> for the default 2× `c6id.2xlarge` in us-east-1). The `ttl_hours` setting is
> only an advisory tag — **nothing auto-reaps**. You are responsible for running
> `make destroy` when finished. `make status` is the cost guard (lists what's
> running). A full provision → bench → destroy cycle is well under an hour, so a
> dollar or two — but a forgotten fleet bills around the clock.

---

## What it does

Two EC2 instances in one VPC/subnet/AZ, in a cluster placement group for a
low-latency private path:

- **node0** — client/driver. Runs the `network-rtt` **client** and the
  single-host benchmarks (`filesystem-write`, `thread-handoff`). Collects results.
- **node1** — the `network-rtt` **responder** (echo server) only.

`network-rtt` is measured **cross-host over the private network** (this is the
whole point — loopback isn't a real network number); single-host benchmarks run
on node0 only.

---

## Setup (one time)

### 1. Install the control-machine tools

These run on the machine that *drives* the rig (your laptop / a control box) —
**not** the provisioned hosts (those get their toolchains from Ansible).

| Tool | Min version | Used by |
|------|-------------|---------|
| `terraform` | >= 1.6 (tested 1.9.8) | `make init/up/destroy/status` |
| `ansible` (ansible-core) | >= 2.16 | `make up/bench` |
| collections `ansible.posix`, `community.general` | latest | `sysctl`, rsync sync |
| `jq` | any | inventory generator + `make status/ssh-node0` |
| `rsync` | any | source sync + result pull |
| an SSH keypair | — | host access |

No root needed (binaries into `~/.local/bin`, which must be on `PATH`):

```bash
mkdir -p ~/.local/bin
curl -fsSL -o /tmp/tf.zip https://releases.hashicorp.com/terraform/1.9.8/terraform_1.9.8_linux_amd64.zip
unzip -o /tmp/tf.zip terraform -d ~/.local/bin
python3 -m pip install --user --break-system-packages ansible-core
ansible-galaxy collection install ansible.posix community.general
sudo apt-get install -y jq rsync          # or `brew install jq rsync` on macOS
```

(The AWS CLI is **not** required — Terraform's AWS provider reads credentials
straight from the environment.)

### 2. Create `.env` — AWS credentials

```bash
cp .env.example .env        # then edit
```

Set either static keys (`AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY`, plus
`AWS_SESSION_TOKEN` for STS) **or** a named profile (`AWS_PROFILE`). The Makefile
auto-loads `.env` and exports it into Terraform/Ansible — no manual `export`.

Use a **scoped IAM principal** (`aws-iam-bench-policy.json` has the policy), not
root account keys. `.env` is gitignored. Prefer bare `KEY=value` (Make keeps
surrounding quotes literally).

### 3. Create `terraform.tfvars` — fleet settings

```bash
cp example.tfvars terraform.tfvars        # then edit
```

> **This step is required and easy to miss.** Terraform only auto-loads a file
> named exactly `terraform.tfvars`; `example.tfvars` is just the template. The
> three variables with no default must be filled:

| Variable | What to put |
|----------|-------------|
| `ssh_public_key` | Your SSH public key, e.g. `ssh-ed25519 AAAA... you@host` (AWS accepts ed25519). |
| `ssh_private_key_file` | Path to the matching private key, e.g. `~/.ssh/id_ed25519`. |
| `allow_ssh_cidr` | **Your** public IP as `/32` — `curl -s https://api.ipify.org` then append `/32`. Never `0.0.0.0/0`. |

Defaults you can usually leave: `node_count = 2`, `instance_type = c6id.2xlarge`,
`region = us-east-1`, `ttl_hours = 4`. **Keep a `c6id`** family — it has the
local NVMe instance store that `filesystem-write` needs (the rig formats and
mounts it at `/opt/bench`). Do **not** use `c7i` (no instance-store NVMe).

---

## Run

```bash
make init        # terraform init (free)
make up          # apply infra + provision hosts (toolchains + build) — ~15-30 min
make bench       # run the matrix on the live hosts, pull results
make destroy     # ⚠️ ALWAYS run when done
```

- **Validate for free first (recommended):** after `make init`, run
  `terraform -chdir=terraform plan -var-file=../terraform.tfvars` — it creates
  nothing and confirms your credentials authenticate and the config is valid.
- **`make up` is slow** (Terraform apply, then Ansible installs Rust/Go/Java and
  builds the benches on both nodes). Expect 15–30 min on a cold fleet.
- **Persistent vs one-shot:**
  - Persistent: `make up` once, then `make bench` as many times as you like
    (each run re-syncs your tree and rebuilds), `make ssh-node0` to poke around,
    `make destroy` when finished.
  - One-shot: `make bench-oneshot` = `up` → `bench` → `destroy`. Note it only
    tears down if every step succeeds — if `up`/`bench` fail, run `make destroy`
    yourself.
- **`make status`** lists the running instances and their uptime — check it
  against `ttl_hours` so you don't forget the teardown.

---

## Results

Each benchmark emits one result-contract JSON line per metric (see
`docs/result-contract.md`). They're appended to `results.jsonl` on node0, then
pulled to the control machine with a run manifest:

```
bench-out/dist/<ts>/results.jsonl     # one JSON object per (cell, metric)
bench-out/dist/<ts>/manifest.txt      # timestamp, instance type, vCPUs, mem,
                                      # kernel, RTT params, git SHA, node roles
```

To track results over time, ingest a run with the `tools/journal` CLI (see the
repo root `README.md` / `CLAUDE.md`): `journal record bench-out/dist/<ts>` then
`journal compare`. The cross-host AWS numbers are the real, reportable figures.

---

## Benchmark matrix

`languages: [rust, go, java]` × `experiments` (in `ansible/group_vars/all.yml`):

| focus_area | experiments | kind |
|------------|-------------|------|
| `network-rtt` | `tcp`, `udp`, `quic` | cross_host (node1 responder + node0 client) |
| `filesystem-write` | `placeholder` (stub) | local (node0, NVMe-backed `/opt/bench`) |
| `thread-handoff` | `placeholder` (stub) | local (node0) |

RTT parameters (`rtt_payload_bytes`, `rtt_warmup`, `rtt_iterations`, and the
per-experiment ports `rtt_tcp_port`/`rtt_udp_port`/`rtt_quic_port`) come from
`group_vars` and are identical across languages so the comparison is fair. Each
cell's artifact is named `<focus_area>-<experiment>` (e.g. `network-rtt-tcp`):
Rust and Go exec the prebuilt release binary
(`rust/target/release/<artifact>`, `go/bin/<artifact>`); Java runs
`./gradlew :<artifact>:run`.

To add an experiment, add a row to `ansible/group_vars/all.yml` `experiments`
(the per-language artifact must already exist — see the repo root `CLAUDE.md`).

---

## Troubleshooting

| Symptom | Cause / fix |
|---------|-------------|
| `make up` fails immediately on auth | `.env` missing or creds empty/quoted. Check `grep -c '^AWS_ACCESS_KEY_ID=..' .env`; use a scoped IAM key. |
| Terraform errors about unset `ssh_public_key` / `allow_ssh_cidr` | No `terraform.tfvars` (only `example.tfvars` exists) — `cp example.tfvars terraform.tfvars` and fill it in. |
| SSH/Ansible times out connecting to hosts | `allow_ssh_cidr` isn't your current public IP. Re-check `curl -s https://api.ipify.org` and update the `/32`, then `make up` again (or update the security group). |
| `InstanceLimitExceeded` / capacity error | Your account's on-demand vCPU limit or AZ capacity. Lower `instance_type`/`node_count` or request a limit increase (still keep a `c6id` family). |
| Benches run but `filesystem-write` is on slow disk | You changed to a non-NVMe instance type. Use `c6id.*`. |
| You're done but unsure if anything is still billing | `make status`; if it lists instances, `make destroy`. When in doubt, destroy. |

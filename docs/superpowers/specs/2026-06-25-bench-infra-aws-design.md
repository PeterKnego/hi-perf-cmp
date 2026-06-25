# bench-infra — AWS Provisioning Rig Design

**Date:** 2026-06-25
**Status:** Proposed — awaiting review

## Purpose

A provisioning rig under `bench-infra/` that spins up a small **AWS** fleet,
installs the three toolchains, builds the benchmarks from the working tree, runs
them, and pulls results back locally. Modeled on `../ultima_cluster/bench-infra/`
(Terraform + Ansible + Makefile + `.env` creds), simplified to this project's
needs: **AWS-only**, **two NVMe-bearing nodes**, polyglot (Rust/Go/Java).

Two hard requirements from the user shape the design:
1. **Network RTT must be cross-host** — measured between two real nodes over the
   private network, never loopback.
2. **Dedicated local NVMe** on each VPS — so `filesystem-write` hits real local
   NVMe, not network-backed EBS.

## Topology

Two EC2 instances in one VPC/subnet/AZ (cluster placement group for a
low-latency private path):

- **node0** — client/driver. Runs the `network-rtt` *client*, and the
  single-host benchmarks (`filesystem-write`, `thread-handoff`).
- **node1** — `network-rtt` *responder* (echo server).

`node_count` defaults to 2 and is parameterizable. Single-host benchmarks run on
node0 only; node1 exists for the RTT loop.

## Part A — Cross-host refactor of `network-rtt` (all three languages)

The benchmark currently runs an in-process loopback echo server. It gains a
**mode selector** (env-driven, consistent with existing `RTT_*` config):

| env var          | values / default                    | meaning                                   |
|------------------|-------------------------------------|-------------------------------------------|
| `RTT_MODE`       | `loopback` (default) \| `server` \| `client` | which role to run                 |
| `RTT_HOST`       | host/IP (client mode)               | responder address to connect to           |
| `RTT_TCP_PORT`   | `9100`                              | TCP echo port (server binds / client dials)|
| `RTT_UDP_PORT`   | `9101`                              | UDP echo port                              |
| `RTT_PAYLOAD_BYTES` / `RTT_WARMUP` / `RTT_ITERATIONS` | 64 / 10000 / 100000 | unchanged           |

Modes:
- **`loopback`** — current behavior: in-process echo server on an ephemeral port
  + client; emits the six result lines. Kept as the local-dev / `cargo run`
  default so nothing regresses locally.
- **`server`** — bind TCP and UDP echo responders on `0.0.0.0` at the configured
  ports and serve until killed. **Emits nothing to stdout** (logs to stderr).
- **`client`** — connect to `RTT_HOST` on both ports, run warmup + measured
  ping-pong, emit the six result lines (`tcp_rtt_*`, `udp_rtt_*`).

Refactor shape (each language): split today's combined echo-server + client loop
into (a) a `serve(addr)` responder and (b) a `client(addr)` measurement loop;
`loopback` wires an in-process `serve` to a `client`, `server`/`client` run one
half. `Stats`, the result contract, TCP_NODELAY, UDP connected-socket + 1s
read-timeout-as-hard-error all carry over unchanged. Echo-byte equality assertion
stays in the client.

This refactor is verified locally (loopback still emits 6 lines; a manual
two-process server/client run on 127.0.0.1 produces sane numbers) before the
infra runs it across hosts.

## Part B — Infrastructure layout

```
bench-infra/
├── Makefile                 # init / up / bench / destroy / status / ssh-node0 / bench-oneshot
├── README.md
├── .env.example             # AWS creds template (.env is gitignored)
├── .gitignore               # .env, inventory/hosts.yml, terraform state, .terraform/
├── example.tfvars           # copy to terraform.tfvars
├── aws-iam-bench-policy.json# least-privilege-ish IAM policy for the rig principal
├── inventory/
│   ├── .gitkeep
│   └── terraform_to_inventory.sh   # terraform output -json → ansible hosts.yml (node0/node1 groups)
├── terraform/
│   ├── versions.tf          # aws provider ~> 5.0, terraform >= 1.6
│   ├── variables.tf         # node_count, instance_type, region, ssh keys, allow_ssh_cidr, ttl_hours, owner
│   ├── main.tf              # VPC, subnet (AZ that offers the type), IGW, RT, SG, key_pair, placement group, instances
│   └── outputs.tf           # nodes [{name,role,public_ip,private_ip}], ssh_user
└── ansible/
    ├── ansible.cfg          # inventory=../inventory/hosts.yml, forks, pipelining
    ├── group_vars/all.yml   # benchmark matrix + RTT params + layout dirs + results paths
    ├── provision.yml        # os_tune → toolchains → build
    ├── bench.yml            # run → collect
    └── roles/
        ├── os_tune/         # NVMe mount, CPU governor, THP off, sysctls, file limits  (carried over)
        ├── toolchains/      # rustup stable, Go 1.22 (tarball→/usr/local/go), JDK 21 (apt)
        ├── build/           # rsync working tree; cargo build --release; go build ./...; ./gradlew build
        ├── run/             # execute the benchmark matrix; files/run_bench.sh wrapper
        └── collect/         # provenance manifest + pull results.jsonl to bench-out/<ts>/
```

Single-AWS (no multi-cloud dummy-provider dance). Terraform AWS module is
inlined at `terraform/` (no per-cloud submodules) since there's only one cloud.

### Terraform (AWS)

Mirrors the reference AWS module: Ubuntu 24.04 AMI (Canonical), `/16` VPC + `/24`
public subnet, instance placed in an **AZ that actually offers the instance
type** (avoids RunInstances "Unsupported"), IGW + route table, security group
(SSH from `allow_ssh_cidr`, **all intra-SG traffic via `self=true`** — covers the
RTT ports), key pair, cluster placement group, `node_count` tagged instances with
static private IPs (`10.10.1.10+i`). Outputs the ordered `nodes` list + `ssh_user`.

Default `instance_type = "c6id.2xlarge"` (8 vCPU, 1× 474 GB local NVMe, sustained
~12.5 Gbps — satisfies both the NVMe and cross-host-bandwidth requirements).
`c7i` is rejected by choice: it has no instance-store NVMe.

### Ansible roles

- **os_tune** — carried over verbatim in intent: detect the instance-store NVMe
  (model "Instance Storage"), `mkfs.ext4` + mount at `{{ remote_home }}` *before*
  anything writes there (so fs-write benchmarks land on NVMe); CPU governor =
  performance; THP off; low-latency sysctls; raised file limits.
- **toolchains** — base apt deps; `openjdk-21-jdk-headless`; rustup stable into
  `{{ remote_home }}/.cargo`; Go 1.22.x from the official tarball into
  `/usr/local/go`; `/etc/profile.d` exports for all three. Records versions for
  provenance.
- **build** — rsync the local working tree to `{{ src_dir }}` (exclude
  `target`, `.git`, `bench-out`, `bench-infra`); `cargo build --release`
  (workspace), `go build ./...`, `./gradlew build` (wrapper is in the synced
  tree). Verify expected artifacts exist. Record git SHA + dirty flag.
- **run** — see matrix below. Ships `files/run_bench.sh` (shellcheck-clean) which,
  given `<language> <focus_area> <mode>` + env, cd's into the synced tree and
  execs the right per-language invocation, printing **only** result-contract JSON
  lines to stdout. Rust/Go run prebuilt release binaries; Java uses
  `./gradlew :<area>:run -q` (quiet → clean stdout). All result lines append to
  `{{ remote_home }}/results/results.jsonl` on node0.
- **collect** — write `manifest.txt` (timestamp, instance type, vCPUs, mem,
  kernel, RTT params, git SHA, both nodes' roles); pull `results/` to
  `bench-out/<ts>/` via rsync.

### Benchmark matrix (`group_vars/all.yml`)

```yaml
languages: [rust, go, java]
focus_areas:
  - { name: network-rtt,      kind: cross_host }
  - { name: filesystem-write, kind: local }
  - { name: thread-handoff,   kind: local }
```

Run-role logic:
- **cross_host** focus area, per language (sequential to avoid port/cross-talk):
  1. start that language's `network-rtt` in **server** mode on **node1**
     (background, fixed `RTT_TCP_PORT`/`RTT_UDP_PORT`),
  2. wait briefly for bind,
  3. run that language's **client** mode on **node0** with
     `RTT_HOST=<node1 private ip>`; append its 6 lines to `results.jsonl`,
  4. kill the node1 server.
- **local** focus area, per language: run on **node0** (writing under the NVMe
  bench home for fs-write); append lines to `results.jsonl`.

RTT params (`RTT_PAYLOAD_BYTES`/`RTT_WARMUP`/`RTT_ITERATIONS`) come from
`group_vars` and are exported into every client run, so all languages use
identical parameters for a fair comparison.

## Makefile targets

`init` (tf init) · `up` (tf apply → inventory → ansible provision) · `bench`
(ansible run+collect) · `bench-oneshot` (up→bench→destroy) · `status` (list
instances + TTL reminder) · `ssh-node0` · `destroy`. `.env` auto-loaded and
exported (AWS creds), same quote-stripping safety net as the reference.

## Credentials & cost

- `.env` (gitignored): `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY` or
  `AWS_PROFILE`. Use a scoped IAM principal (`aws-iam-bench-policy.json`), not
  root keys.
- `ttl_hours` is an **advisory tag only** — nothing auto-reaps; `make destroy`
  is the teardown. `make status` is the cost guard.

## Testing / verification

- **network-rtt refactor:** `cargo test` (Stats) still green in all langs;
  loopback mode still emits 6 lines locally; a local two-terminal
  server↔client run on 127.0.0.1 yields sane cross-process numbers, per language.
- **infra:** `terraform validate` + `terraform plan` clean; `run_bench.sh`
  passes `shellcheck`; inventory script produces valid YAML from sample tf
  output. A real `make bench-oneshot` (paid, user-initiated) is the end-to-end
  check — not run automatically.

## Out of scope (YAGNI)

- GCP/Hetzner modules (AWS-only).
- >2 nodes / fan-out topologies.
- Auto-reaping/scheduling of instances.
- Result aggregation/plotting across runs — that's the `harness/` placeholder's
  eventual job; it will consume the `results.jsonl` this rig produces.

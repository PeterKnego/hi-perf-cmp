# CLAUDE.md — bench-infra

Guidance for Claude Code (and any agent) operating in `bench-infra/`. This is the
**paid, outward-facing** part of the repo: it provisions real AWS EC2 instances.
Read `bench-infra/README.md` for the full human-facing walkthrough; this file is
the operating rules and the gotchas that aren't obvious from the README.

## Non-negotiable safety rules

1. **`make up`, `make bench-oneshot`, and `terraform apply` cost real money and
   are USER-INITIATED.** Never run them on your own initiative. Provision only
   when the user has explicitly asked for a run in this session. Spinning up the
   default fleet is ~**$0.81/hr** (2× `c6id.2xlarge`, us-east-1 on-demand).
2. **Nothing auto-reaps.** `ttl_hours` is an advisory tag only. After ANY run you
   start, you MUST `make destroy` and confirm with `make status` that nothing is
   left running — even if the bench step failed. Leaving instances up burns money
   silently.
3. **`make bench-oneshot` is NOT teardown-safe.** It's `up bench destroy` as Make
   prerequisites, so if `up` or `bench` fails, Make stops and `destroy` never
   runs. For anything that might fail, run the steps separately and always run
   `make destroy` yourself (in a `finally`-style step), success or failure.
4. **Validate for free before you spend.** `make init` then
   `terraform -chdir=terraform plan -var-file=../terraform.tfvars` creates
   nothing and proves the creds authenticate and the config is valid. Do this
   before `make up`.
5. **Never commit or print secrets.** `.env` and `terraform.tfvars` are
   gitignored — keep them that way. Don't `cat` `.env`; check var presence with
   `grep -c` instead.

## The two user-supplied files (the #1 setup gotcha)

Neither is committed; both must exist before `make up`:

- **`.env`** — AWS credentials. `cp .env.example .env`, then set
  `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` (or `AWS_PROFILE`). The Makefile
  auto-loads it (Make `-include`, not shell-sourced) and exports into
  terraform/ansible. Use bare `KEY=value` (Make keeps quotes literally; the
  Makefile strips quotes from the known cred vars as a safety net).
- **`terraform.tfvars`** — `cp example.tfvars terraform.tfvars`, then fill the
  three variables that have **no default**: `ssh_public_key`,
  `ssh_private_key_file`, `allow_ssh_cidr`. Terraform only auto-loads a file
  named exactly `terraform.tfvars`, and the Makefile applies
  `-var-file=../terraform.tfvars`. `example.tfvars` alone does nothing — a
  missing `terraform.tfvars` is the most common "it won't run" cause.
  - `ssh_public_key` / `ssh_private_key_file` — an existing keypair (AWS accepts
    ed25519). The module imports the public key; the private key path is written
    into the Ansible inventory for host access.
  - `allow_ssh_cidr` — the **control machine's public IP /32** (the box running
    Terraform/Ansible — i.e. wherever this runs). Get it with
    `curl -s https://api.ipify.org`. Never `0.0.0.0/0`.

## Workflow

```
make init                      # terraform init (free)
make up                        # apply + inventory + ansible provision (LONG)
make bench                     # run the matrix on the live hosts, pull results
make destroy                   # ALWAYS, when done
make status                    # cost guard: lists instances + uptime
```

- **`make up` is long** (~15–30 min): Terraform apply (~2–3 min) then Ansible
  installs Rust/Go/Java on both nodes and builds the benches from the synced
  local tree. **Run it in the background** (it exceeds normal command timeouts)
  and watch `bench-out`/the make output. `make bench` is shorter but still runs
  the full cross-host matrix.
- Persistent loop: `make up` once → `make bench` repeatedly (re-syncs + rebuilds)
  → `make ssh-node0` to investigate → `make destroy` when done.

## What the matrix runs

Defined in `ansible/group_vars/all.yml` (`languages` × `experiments`). Currently:
`network-rtt` × {`tcp`, `udp`, `quic`} cross-host (node1 responder + node0
client, real private-network RTT — never loopback), plus the `filesystem-write`
(`fsync`/`fdatasync`/`prealloc`/`batch`) and `thread-handoff`
(`spin`/`condvar`/`channel`/`ring`) local experiments. RTT params (`rtt_payload_bytes`, `rtt_warmup`,
`rtt_iterations`, the per-experiment ports) are identical across languages for a
fair comparison. Each cell's artifact is `<focus_area>-<experiment>` (e.g.
`network-rtt-tcp`): Rust/Go exec the prebuilt release binary, Java runs
`./gradlew :<artifact>:run`.

Results land in `bench-out/dist/<ts>/{results.jsonl,manifest.txt}` on the control
machine (the result-contract lines + a run manifest with instance type, kernel,
git SHA, node roles).

## After a run: record it

The AWS cross-host numbers are the **real, reportable** figures (loopback runs in
the autobench loop are local fitness only). Record the run with the `tools/journal`
CLI into `journal/runs/<ts>-<sha>/` against the producing commit, then
`journal compare` vs the baseline. The first *real* journal entries should come
from genuine cross-host AWS runs like these, not loopback.

## Editing the rig

- Add an experiment to the matrix: a row in `ansible/group_vars/all.yml`
  `experiments` (plus the per-language artifact must exist — see the root
  `CLAUDE.md` "Adding an experiment").
- Change instance size/region/topology in `terraform.tfvars`. Keep a
  **local-NVMe** instance family (`c6id`, not `c7i`) — `filesystem-write` needs
  the instance store, which the `os_tune` role formats and mounts at `/opt/bench`.
- Terraform state lives in `terraform/terraform.tfstate` (gitignored, local
  backend). Don't delete it while instances exist or you'll orphan billable
  resources — `make destroy` first.

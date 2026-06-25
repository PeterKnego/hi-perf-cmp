# Load AWS credentials from a gitignored .env if present, and export them into
# every recipe's environment so terraform/ansible pick them up automatically.
# Leading '-' makes .env optional. .env lines are KEY=value (bare value best).
-include .env
# Safety net: make's -include keeps surrounding quotes LITERALLY (unlike shell
# dotenv loaders), so a value like AWS_SECRET_ACCESS_KEY="abc" would be passed
# to terraform as `"abc"` (with quotes) and fail auth. Strip surrounding double
# quotes from known cred vars so both `KEY=abc` and `KEY="abc"` work.
_ENV_CREDS := AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN AWS_PROFILE
$(foreach v,$(_ENV_CREDS),$(eval $(v) := $$(patsubst "%",%,$$($(v)))))
export

TF      := terraform -chdir=terraform
TFVARS  ?= terraform.tfvars
ANSIBLE := ansible-playbook
SSH_KEY := $(shell awk -F'"' '/ssh_private_key_file/{print $$2}' $(TFVARS) 2>/dev/null)

.PHONY: init up provision bench bench-oneshot status destroy ssh-node0 inventory

init: ## terraform init
	$(TF) init

up: ## provision infra + configure hosts (tf apply -> inventory -> ansible provision)
	$(TF) apply -auto-approve -var-file=../$(TFVARS)
	$(MAKE) inventory
	cd ansible && $(ANSIBLE) provision.yml

inventory:
	SSH_PRIVATE_KEY_FILE=$(SSH_KEY) ./inventory/terraform_to_inventory.sh

bench: ## run the benchmark matrix + collect (hosts must be up)
	cd ansible && $(ANSIBLE) bench.yml

bench-oneshot: up bench destroy ## clean-room one-shot run

status: ## list instances + TTL reminder (cost guard)
	@$(TF) output -json nodes | jq -r '.[] | "\(.name)\t\(.role)\t\(.public_ip)\t\(.private_ip)"'
	@echo "TTL guard: nothing auto-reaps. Check uptime vs ttl_hours in $(TFVARS); run 'make destroy' when done."
	@cd ansible && ansible cluster -m shell -a "echo $$(hostname): up $$(awk '{print int($$1/3600)}' /proc/uptime)h" 2>/dev/null || true

destroy: ## tear everything down
	$(TF) destroy -auto-approve -var-file=../$(TFVARS)
	rm -f inventory/hosts.yml

ssh-node0:
	@ip=$$($(TF) output -json nodes | jq -r '.[]|select(.role=="node0").public_ip'); \
	 user=$$($(TF) output -raw ssh_user); ssh -i $(SSH_KEY) $$user@$$ip

.PHONY: test-provider-formats update-provider-format-baselines

# Check locally installed Providers against approved baselines and real adapters.
test-provider-formats:
	cargo test --lib --all-features local_format_compatibility -- --ignored --nocapture --test-threads=1

# Explicitly extend JSONL baselines and replace exact schema baselines.
update-provider-format-baselines:
	CODING_AGENT_DATA_UPDATE_FORMAT_BASELINES=1 cargo test --lib --all-features local_format_compatibility -- --ignored --nocapture --test-threads=1

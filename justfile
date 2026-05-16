# plasm-core (OSS subtree): plugin packing only. For Phoenix + SaaS Tool Explorer, use the plasm monorepo root — `just local-web`.

set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

root := justfile_directory()
export PATH := env_var_or_default("PATH", "/usr/bin:/bin")

default:
	@just --list

# Pack apis/* into target/plasm-plugins (plugin stubs default to Cargo release; PLASM_OSS_RUST_DEBUG=1 uses debug for packer binary and `--debug` for stubs). Fails if no dylibs produced.
pack-plugins:
	bash -c 'set -euo pipefail; cd "{{root}}"; mkdir -p "{{root}}/target/plasm-plugins"; _cr=(); [[ -z "$${PLASM_OSS_RUST_DEBUG:-}" ]] && _cr=(--release); _pack=(); [[ -n "$${PLASM_OSS_RUST_DEBUG:-}" ]] && _pack=(--debug); cargo run "$${_cr[@]}" -p plasm --bin plasm-pack-plugins -- --workspace "{{root}}" --apis-root "{{root}}/apis" --output-dir "{{root}}/target/plasm-plugins" "$${_pack[@]}"; if ! find "{{root}}/target/plasm-plugins" -maxdepth 1 \( -name "libplasm_plugin_*.dylib" -o -name "libplasm_plugin_*.so" -o -name "libplasm_plugin_*.dll" \) | grep -q .; then echo "pack-plugins: no dylibs in {{root}}/target/plasm-plugins — apis/ may be empty (init submodule / catalogs)." >&2; exit 1; fi'

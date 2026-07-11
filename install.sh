#!/usr/bin/env bash
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

info()  { echo -e "${BLUE}ℹ${NC}  $*"; }
ok()    { echo -e "${GREEN}✓${NC}  $*"; }
warn()  { echo -e "${YELLOW}⚠${NC}  $*"; }
err()   { echo -e "${RED}✗${NC}  $*" >&2; }
step()  { echo -e "\n${CYAN}${BOLD}── $* ──${NC}"; }

REPO="ishan-parihar/tdg-rust"
GITHUB_URL="https://github.com/${REPO}/releases/latest/download/tdg-rust"

detect_hermes_home() {
    if [[ -n "${HERMES_HOME:-}" ]]; then return; fi
    if command -v hermes &>/dev/null; then
        local detected
        detected=$(hermes config get home 2>/dev/null || true)
        if [[ -n "$detected" && -d "$detected" ]]; then
            HERMES_HOME="$detected"
            return
        fi
    fi
    HERMES_HOME="$HOME/.hermes"
}

detect_arch() {
    local arch
    arch=$(uname -m)
    case "$arch" in
        x86_64|amd64)  ARCH="x86_64" ;;
        aarch64|arm64) ARCH="aarch64" ;;
        *) err "Unsupported architecture: $arch"; exit 1 ;;
    esac
}

do_uninstall() {
    step "Uninstalling TDG-Rust"
    local tdg_dir="${HERMES_HOME}/tdg-rust"
    local plugin_dir="${HERMES_HOME}/plugins/tdg"
    local config="${HERMES_HOME}/config.yaml"

    [[ -f "$tdg_dir/tdg-rust" ]] && rm "$tdg_dir/tdg-rust" && ok "Removed binary"
    [[ -d "$tdg_dir/lib" ]] && rm -rf "$tdg_dir/lib" && ok "Removed ONNX runtime library"
    [[ -d "$tdg_dir" ]] && rm -rf "$tdg_dir" && ok "Removed $tdg_dir"
    [[ -d "$plugin_dir" ]] && rm -rf "$plugin_dir" && ok "Removed plugin: $plugin_dir"

    if [[ -f "$config" ]]; then
        python3 -c "
import re
with open('$config') as f: content = f.read()
content = re.sub(r'  tdg:\n(?:    [^\n]+\n)*', '', content)
content = content.replace('  provider: tdg\n', '')
with open('$config', 'w') as f: f.write(content)
" 2>/dev/null && ok "Patched config.yaml" || warn "Could not auto-patch config"
    fi

    ok "TDG-Rust uninstalled. Restart gateway."
    exit 0
}

download_binary() {
    local tdg_dir="${HERMES_HOME}/tdg-rust"
    local bin_path="${tdg_dir}/tdg-rust"

    step "Downloading tdg-rust binary"

    if [[ -f "$bin_path" ]]; then
        local current_version
        current_version=$("$bin_path" --version 2>/dev/null | awk '{print $2}' || echo "unknown")
        ok "tdg-rust $current_version already installed at $bin_path"
        read -rp "$(echo -e "${YELLOW}?${NC}  Re-download latest? [y/N]: ")" answer
        if [[ ! "$answer" =~ ^[Yy]$ ]]; then
            return
        fi
    fi

    mkdir -p "$tdg_dir"

    info "Downloading from $GITHUB_URL..."
    if command -v curl &>/dev/null; then
        curl -fsSL "$GITHUB_URL" -o "$bin_path"
    elif command -v wget &>/dev/null; then
        wget -q "$GITHUB_URL" -O "$bin_path"
    else
        err "curl or wget required"
        exit 1
    fi

    chmod +x "$bin_path"
    local version
    version=$("$bin_path" --version 2>/dev/null || echo "unknown")
    ok "Downloaded tdg-rust $version"
}

download_onnx_runtime() {
    local tdg_dir="${HERMES_HOME}/tdg-rust"
    local lib_dir="${tdg_dir}/lib"
    local ort_version="1.20.1"
    # Map detected architecture to ONNX Runtime archive naming
    local ort_arch
    case "${ARCH}" in
        x86_64)  ort_arch="x64" ;;
        aarch64) ort_arch="aarch64" ;;
        *)       ort_arch="x64"; warn "Unknown arch ${ARCH}, defaulting to x64" ;;
    esac
    local ort_url="https://github.com/microsoft/onnxruntime/releases/download/v${ort_version}/onnxruntime-linux-${ort_arch}-${ort_version}.tgz"

    if [[ -f "${lib_dir}/libonnxruntime.so.1.20.1" ]] || [[ -f "${lib_dir}/libonnxruntime.so.${ort_version}" ]]; then
        ok "ONNX Runtime library already installed"
        return
    fi

    step "Downloading ONNX Runtime ${ort_version}"
    mkdir -p "$lib_dir"

    local tmp_dir
    tmp_dir=$(mktemp -d)

    info "Downloading from $ort_url..."
    if command -v curl &>/dev/null; then
        curl -fsSL "$ort_url" -o "${tmp_dir}/ort.tgz"
    elif command -v wget &>/dev/null; then
        wget -q "$ort_url" -O "${tmp_dir}/ort.tgz"
    else
        err "curl or wget required"
        exit 1
    fi

    info "Extracting..."
    tar xzf "${tmp_dir}/ort.tgz" -C "${tmp_dir}"

    local found=0
    while IFS= read -r -d '' so_file; do
        cp "$so_file" "$lib_dir/"
        found=1
        break
    done < <(find "${tmp_dir}" -name "libonnxruntime.so*" -type f -print0 2>/dev/null)

    if [[ "$found" -eq 0 ]]; then
        warn "Could not find libonnxruntime.so in archive"
        rm -rf "$tmp_dir"
        return
    fi

    local real_so
    real_so=$(ls "${lib_dir}"/libonnxruntime.so.*.*.* 2>/dev/null | head -1)
    if [[ -n "$real_so" ]]; then
        local basename_so
        basename_so=$(basename "$real_so")
        local major="${basename_so#libonnxruntime.so.}"
        major="${major%%.*}"
        ln -sf "$basename_so" "${lib_dir}/libonnxruntime.so.${major}"
        ln -sf "$basename_so" "${lib_dir}/libonnxruntime.so"
    fi

    rm -rf "$tmp_dir"
    ok "ONNX Runtime ${ort_version} installed at $lib_dir"
}

install_adapter() {
    local plugins_dir="${HERMES_HOME}/plugins"
    local plugin_dir="${plugins_dir}/tdg"

    step "Installing Python adapter"

    mkdir -p "$plugin_dir"

    local adapter_url="https://raw.githubusercontent.com/${REPO}/main/plugins/tdg/__init__.py"
    local plugin_url="https://raw.githubusercontent.com/${REPO}/main/plugins/tdg/plugin.yaml"

    info "Downloading adapter files..."
    if command -v curl &>/dev/null; then
        curl -fsSL "$adapter_url" -o "${plugin_dir}/__init__.py"
        curl -fsSL "$plugin_url" -o "${plugin_dir}/plugin.yaml"
    else
        wget -q "$adapter_url" -O "${plugin_dir}/__init__.py"
        wget -q "$plugin_url" -O "${plugin_dir}/plugin.yaml"
    fi

    ok "Adapter installed at $plugin_dir"
}

init_database() {
    local tdg_dir="${HERMES_HOME}/tdg-rust"
    local db_dir="${HERMES_HOME}/tdg"
    local db_path="${db_dir}/graph.db"

    step "Initializing database"

    mkdir -p "$db_dir"

    if [[ -f "$db_path" ]]; then
        local size
        size=$(stat -c%s "$db_path" 2>/dev/null || stat -f%z "$db_path" 2>/dev/null || echo 0)
        local size_mb=$((size / 1048576))
        ok "graph.db exists (${size_mb}MB) — skipping init"
    else
        info "Creating fresh graph.db..."
        TDG_HOME="$HERMES_HOME" "$tdg_dir/tdg-rust" init 2>&1 | tail -3
        ok "graph.db initialized"
    fi
}

patch_config() {
    if [[ "${TDG_SKIP_CONFIG:-0}" == "1" ]]; then
        info "Skipping config patching"
        return
    fi

    local config="${HERMES_HOME}/config.yaml"
    if [[ ! -f "$config" ]]; then
        warn "config.yaml not found — skipping"
        return
    fi

    step "Patching Hermes configuration"

    local tdg_bin="${HERMES_HOME}/tdg-rust/tdg-rust"
    local tdg_lib="${HERMES_HOME}/tdg-rust/lib"

    python3 -c "
import re

config_path = '$config'
tdg_bin = '$tdg_bin'
hermes_home = '$HERMES_HOME'

with open(config_path, 'r') as f:
    content = f.read()

changes = []

mcp_block = '''  tdg:
    command: ''' + tdg_bin + '''
    args:
    - serve
    env:
      TDG_HOME: ''' + hermes_home + '''
      NO_COLOR: '1'
      LD_LIBRARY_PATH: ''' + tdg_lib + '''
    connect_timeout: 30
    timeout: 120
'''

if '  tdg:' not in content:
    marker = 'platform_toolsets:'
    if marker in content:
        content = content.replace(marker, mcp_block + marker)
        changes.append('Added MCP server block')
    else:
        content += '\n' + mcp_block
        changes.append('Appended MCP server block')
else:
    pattern = r'  tdg:\n(?:    [^\n]*\n)*'
    content = re.sub(pattern, mcp_block.lstrip('\n'), content)
    changes.append('Updated MCP server block')

if 'provider: tdg' not in content:
    if re.search(r'memory:\s*\n(?:.*\n)*?\s+provider:\s+\w+', content):
        content = re.sub(
            r'(memory:\s*\n(?:\s+.+\n)*?\s+provider:)\s+\w+',
            r'\1 tdg',
            content
        )
        changes.append('Set memory.provider = tdg')
    elif 'memory:' in content:
        content = re.sub(r'(memory:\s*\n)', r'\1  provider: tdg\n', content)
        changes.append('Added memory.provider = tdg')

with open(config_path, 'w') as f:
    f.write(content)

print('Config changes:')
for c in changes:
    print(f'  • {c}')
" 2>&1

    ok "Configuration updated"
}

create_wrapper() {
    local tdg_dir="${HERMES_HOME}/tdg-rust"
    local wrapper_path="${tdg_dir}/tdg"

    step "Creating wrapper script"

    cat << 'EOF' > "$wrapper_path"
#!/usr/bin/env bash
# Wrapper to run tdg-rust with the correct LD_LIBRARY_PATH for ONNX
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export LD_LIBRARY_PATH="$DIR/lib:$LD_LIBRARY_PATH"
exec "$DIR/tdg-rust" "$@"
EOF

    chmod +x "$wrapper_path"
    ok "Created helper wrapper script at $wrapper_path"
}

post_install() {
    local tdg_dir="${HERMES_HOME}/tdg-rust"

    echo ""
    echo -e "${GREEN}${BOLD}═══════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}${BOLD}  TDG-Rust installed successfully!${NC}"
    echo -e "${GREEN}${BOLD}═══════════════════════════════════════════════════════════${NC}"
    echo ""
    echo -e "  ${BOLD}Wrapper Command:${NC} $tdg_dir/tdg"
    echo -e "  ${BOLD}Rust Binary:${NC}     $tdg_dir/tdg-rust"
    echo -e "  ${BOLD}Database:${NC}        $HERMES_HOME/tdg/graph.db"
    echo -e "  ${BOLD}Adapter:${NC}         $HERMES_HOME/plugins/tdg/"
    echo ""
    echo -e "  ${BOLD}What's installed:${NC}"
    echo -e "    • 33 MCP tools via Rust binary (zero Python dependency)"
    echo -e "    • Graph-enhanced retrieval with 1-hop expansion"
    echo -e "    • Entity resolution and confidence decay"
    echo -e "    • Inline embedding generation"
    echo ""
    echo -e "  ${BOLD}Next steps:${NC}"
    echo -e "    1. ${CYAN}Restart your Hermes gateway${NC}:"
    echo -e "       hermes gateway restart"
    echo -e ""
    echo -e "    2. ${CYAN}Verify installation${NC}:"
    echo -e "       $tdg_dir/tdg --version"
    echo -e "       $tdg_dir/tdg stats"
    echo -e ""
    echo -e "  ${BOLD}Uninstall:${NC}   TDG_UNINSTALL=1 bash $0"
    echo ""
}

main() {
    echo -e "${BOLD}${CYAN}"
    echo "  ╔═══════════════════════════════════════════════╗"
    echo "  ║  TDG-Rust Installer for Hermes Agent         ║"
    echo "  ║  High-performance graph memory infrastructure ║"
    echo "  ╚═══════════════════════════════════════════════╝"
    echo -e "${NC}"

    for arg in "$@"; do
        case "$arg" in
            --uninstall) TDG_UNINSTALL=1 ;;
            --skip-config) TDG_SKIP_CONFIG=1 ;;
            --help|-h)
                echo "Usage: bash install.sh [--uninstall] [--skip-config]"
                echo ""
                echo "Options:"
                echo "  --uninstall     Remove TDG-Rust from this system"
                echo "  --skip-config   Don't modify config.yaml"
                echo ""
                echo "Environment:"
                echo "  HERMES_HOME     Override Hermes home directory"
                exit 0
                ;;
        esac
    done

    detect_hermes_home
    detect_arch
    info "HERMES_HOME: $HERMES_HOME"
    info "Architecture: $ARCH"

    if [[ ! -d "$HERMES_HOME" ]]; then
        err "HERMES_HOME not found: $HERMES_HOME"
        err "Install Hermes Agent first: https://github.com/NousResearch/hermes-agent"
        exit 1
    fi

    [[ "${TDG_UNINSTALL:-0}" == "1" ]] && do_uninstall

    download_binary
    download_onnx_runtime
    create_wrapper
    install_adapter
    init_database
    patch_config
    post_install
}

main "$@"

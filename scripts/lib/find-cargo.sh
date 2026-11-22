# Sourced by the scripts that build the server. Shells that skip the login
# profile (agents, git hooks, GUI git clients) may not have cargo on PATH
# even though rustup is installed, as with Homebrew's keg-only rustup.
# rustup knows where its cargo is.
if ! command -v cargo >/dev/null; then
  if command -v rustup >/dev/null; then
    PATH="$(dirname "$(rustup which cargo)"):$PATH"
  else
    echo "cargo is not on PATH and rustup is not installed. Install Rust with rustup: https://rustup.rs" >&2
    exit 1
  fi
fi

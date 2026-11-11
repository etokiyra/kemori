#!/bin/sh
# Scaffold the throwaway project used by docs/demo.tape, then record the README
# GIF with VHS (https://github.com/charmbracelet/vhs).
#
#   ./scripts/demo.sh                 # scaffold only
#   ./scripts/demo.sh --record        # scaffold and record docs/images/demo.gif
#
# The project lives in /tmp/kemori-demo so it never touches the repository.
set -eu

DEMO_DIR="${KEMORI_DEMO_DIR:-/tmp/kemori-demo/aurora}"

mkdir -p "$DEMO_DIR/src"

cat > "$DEMO_DIR/Cargo.toml" <<'EOF'
[package]
name = "aurora"
version = "0.1.0"
edition = "2021"
EOF

cat > "$DEMO_DIR/README.md" <<'EOF'
# aurora

A tiny demo project.
EOF

printf 'target/\n' > "$DEMO_DIR/.gitignore"

cat > "$DEMO_DIR/src/main.rs" <<'EOF'
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct Star {
    name: String,
    brightness: f64,
}

impl Star {
    fn new(name: &str, brightness: f64) -> Self {
        Self { name: name.to_string(), brightness }
    }
}

fn constellation(stars: &[Star]) -> HashMap<char, f64> {
    let mut map: HashMap<char, f64> = HashMap::new();
    for star in stars {
        let initial = star.name.chars().next().unwrap_or('?');
        *map.entry(initial).or_insert(0.0) += star.brightness;
    }
    map
}

fn main() {
    let stars = vec![
        Star::new("Vega", 0.03),
        Star::new("Deneb", 1.25),
        Star::new("Altair", 0.77),
    ];
    let map = constellation(&stars);
    println!("{map:?}");
}
EOF

(cd "$DEMO_DIR" && git init -q 2>/dev/null || true)

echo "demo project ready: $DEMO_DIR"

if [ "${1:-}" = "--record" ]; then
    command -v vhs >/dev/null 2>&1 || {
        echo "error: vhs is not installed (https://github.com/charmbracelet/vhs)" >&2
        exit 1
    }
    mkdir -p docs/images
    vhs docs/demo.tape
    echo "wrote docs/images/demo.gif"
fi

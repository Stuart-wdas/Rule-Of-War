"""Validate self-contained terrain sampling helpers with Bevy's actual Naga.

This catches WGSL syntax, built-in spelling and texture/vector types quickly.
The full Bevy PBR composition still requires check_ground_demo.py's native
render checks. Uses the project's already compiled transitive Naga dependency;
does not change Cargo dependencies or shader sources.
"""
from pathlib import Path
import json
import subprocess

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.data/render-checks/ground-demo/shader-helpers'
DEPS = ROOT / 'native/target/debug/deps'

def main():
    OUT.mkdir(parents=True, exist_ok=True)
    source = (ROOT / 'native/src/grass_blend.wgsl').read_text()
    helpers = source[source.index('@group(#{MATERIAL_BIND_GROUP})'):source.index('@fragment')]
    helpers = helpers.replace('#{MATERIAL_BIND_GROUP}', '0')
    (OUT / 'helpers.wgsl').write_text(helpers)
    rust = OUT / 'validate.rs'
    rust.write_text('''fn main() {
    let path = std::env::args().nth(1).unwrap();
    let source = std::fs::read_to_string(path).unwrap();
    let module = naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&source)));
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
        .validate(&module).unwrap();
    println!("Terrain sampling helpers passed Naga parsing and validation");
}''')
    naga = max(DEPS.glob('libnaga-*.rlib'), key=lambda p: p.stat().st_mtime)
    exe = OUT / 'validate.exe'
    subprocess.run(['rustc', str(rust), '--edition=2024', '--extern', f'naga={naga}',
                    '-L', f'dependency={DEPS}', '-o', str(exe)], cwd=ROOT, check=True)
    checked = subprocess.run([str(exe), str(OUT / 'helpers.wgsl')], check=True, text=True, capture_output=True)
    print(checked.stdout, end='')
    broken = OUT / 'invalid-builtin.wgsl'
    broken.write_text(helpers.replace('/ sqrt(dot(w,w))', '* inversesqrt(dot(w,w))'))
    rejected = subprocess.run([str(exe), str(broken)], text=True, capture_output=True)
    assert rejected.returncode != 0 and 'inversesqrt' in rejected.stderr
    (OUT / 'validation.json').write_text(json.dumps({'validated': True, 'rejectsLegacyBuiltinSpelling': True, 'scope': 'standalone terrain texture sampling helpers', 'validator': str(naga)}, indent=2))

if __name__ == '__main__':
    main()

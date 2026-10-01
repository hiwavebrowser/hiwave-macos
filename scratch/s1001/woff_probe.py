"""Queue item 2, first question: does Core Graphics (`CGFont::from_data_provider`, what webfonts.rs
installs with) accept WOFF / WOFF2 bytes as they are? Makes Ahem.woff / Ahem.woff2 with fontTools,
appends a throwaway test to develop's webfonts.rs, runs it, restores the file."""
import subprocess
from fontTools.ttLib import TTFont
HUB = '/Users/petecopeland/Repos/.worktrees/trench-realsite'
WT = '/Users/petecopeland/Repos/.worktrees/rs-dev-9f49a40'
SRC = f'{WT}/crates/rustkit-text/src/webfonts.rs'
ttf = f'{WT}/crates/rustkit-text/tests/fixtures/Ahem.ttf'
for flavor in ('woff', 'woff2'):
    f = TTFont(ttf)
    f.flavor = flavor
    f.save(f'{HUB}/scratch/s1001/Ahem.{flavor}')
    print(flavor, len(open(f'{HUB}/scratch/s1001/Ahem.{flavor}', 'rb').read()), 'bytes')
orig = open(SRC).read()
probe = '''
#[cfg(all(test, target_os = "macos"))]
mod woff_probe {
    use core_graphics::data_provider::CGDataProvider;
    use core_graphics::font::CGFont;
    use std::sync::Arc;

    #[test]
    fn does_core_graphics_take_woff_bytes() {
        for (name, bytes) in [
            ("ttf", &include_bytes!("../tests/fixtures/Ahem.ttf")[..]),
            ("woff", &include_bytes!("%s/scratch/s1001/Ahem.woff")[..]),
            ("woff2", &include_bytes!("%s/scratch/s1001/Ahem.woff2")[..]),
        ] {
            let provider = CGDataProvider::from_buffer(Arc::new(bytes.to_vec()));
            let ok = CGFont::from_data_provider(provider).is_ok();
            println!("DBG cgfont accepts {name}: {ok} ({} bytes, magic {:?})", bytes.len(), &bytes[..4]);
        }
    }
}
''' % (HUB, HUB)
open(SRC, 'w').write(orig + probe)
try:
    r = subprocess.run(['python3', f'{HUB}/scratch/s0930/ctest.py', WT, '-p', 'rustkit-text', '--lib', '--',
                        'woff_probe', '--nocapture'], capture_output=True, text=True, timeout=900)
    print(r.stdout[-3000:], r.stderr[-1500:])
finally:
    open(SRC, 'w').write(orig)
    print(subprocess.run(['git', 'status', '--short'], cwd=WT, capture_output=True, text=True).stdout or 'develop worktree clean')

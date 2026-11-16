//! W0 spike: do the five JetStream wasm binaries load in wasmi? Stub imports, no glue.
//! Usage: load5 <dir holding quicksort.wasm, richards.wasm, gcc-loops.wasm, HashSet.wasm, tsf.wasm>
//! (the pinned JetStream checkout's `wasm/` directory).
use std::time::Instant;
use wasmi::*;

fn main() {
    let dir = std::env::args().nth(1).expect("usage: load5 <JetStream wasm directory>");
    for f in ["quicksort", "richards", "gcc-loops", "HashSet", "tsf"] {
        let bytes = std::fs::read(format!("{dir}/{f}.wasm")).unwrap();
        let engine = Engine::default();
        let t0 = Instant::now();
        let module = match Module::new(&engine, &bytes[..]) { Ok(m) => m, Err(e) => { println!("{f}: COMPILE ERROR {e}"); continue } };
        let compile = t0.elapsed();
        let mut store = Store::new(&engine, ());
        let mut linker = <Linker<()>>::new(&engine);
        let (mut nf, mut nm, mut nt, mut ng) = (0, 0, 0, 0);
        for imp in module.imports() {
            let ext: Extern = match imp.ty() {
                ExternType::Func(ft) => { nf += 1; Func::new(&mut store, ft.clone(), |_, _, res| { for r in res.iter_mut() { *r = Val::default(r.ty()); } Ok(()) }).into() }
                ExternType::Memory(mt) => { nm += 1; Memory::new(&mut store, *mt).unwrap().into() }
                ExternType::Table(tt) => { nt += 1; Table::new(&mut store, *tt, Ref::null(tt.element())).unwrap().into() }
                ExternType::Global(gt) => { ng += 1; Global::new(&mut store, Val::default(gt.content()), gt.mutability()).into() }
                _ => panic!("unexpected import kind"),
            };
            linker.define(imp.module(), imp.name(), ext).unwrap();
        }
        let t1 = Instant::now();
        let inst = linker.instantiate_and_start(&mut store, &module);
        let inst_t = t1.elapsed();
        match inst {
            Ok(i) => {
                let nexp = module.exports().count();
                println!("{f:10} {:7} bytes: compile {:7.1} ms, instantiate {:6.1} ms; imports {nf} func {nm} memory {nt} table {ng} global; {nexp} exports; instantiated OK ({} funcs exported)",
                    bytes.len(), compile.as_secs_f64() * 1e3, inst_t.as_secs_f64() * 1e3, i.exports(&store).filter(|e| e.clone().into_func().is_some()).count());
            }
            Err(e) => println!("{f}: INSTANTIATE ERROR {e}"),
        }
    }
}

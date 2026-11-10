//! E54b: which image an `<img>` loads (HTML "select an image source", §4.8.4.3): the `<picture>`
//! sources, `srcset` with `x` and `w` descriptors, `sizes`, `type` and `media`, and the density of
//! the pick (so the natural size can be corrected by it).
//!
//! Pure functions over attribute strings, so the whole selection is unit-tested against what
//! Chrome answers on a matrix page (`docs/diagnostics/2026-11-10/e54b_srcset_matrix.md`).

/// What the selection reads from one source of images: an `<img>` or a `<picture>`'s `<source>`.
#[derive(Debug, Default, Clone)]
pub struct SourceAttrs<'a> {
    pub srcset: Option<&'a str>,
    pub sizes: Option<&'a str>,
    pub media: Option<&'a str>,
    pub mime: Option<&'a str>,
    /// Only the `<img>`'s own `src` (a `<source>` has none).
    pub src: Option<&'a str>,
}

/// The environment a selection is made in.
#[derive(Debug, Clone, Copy)]
pub struct Env {
    pub viewport_w: f32,
    pub viewport_h: f32,
    pub dpr: f32,
}

/// The chosen image and its density: a decoded image of N pixels is N / `density` CSS px wide.
#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    pub url: String,
    pub density: f32,
}

/// Select the image for an `<img>` whose `<picture>` sources (those before it) are `sources`.
pub fn select_source(img: &SourceAttrs<'_>, sources: &[SourceAttrs<'_>], env: &Env) -> Option<Pick> {
    // STUB (red): the old behaviour, the densest or widest candidate of the img's own srcset.
    let _ = (sources, env);
    let srcset = img.srcset?;
    let mut best: Option<(f32, String)> = None;
    for cand in srcset.split(',') {
        let mut parts = cand.split_whitespace();
        let Some(url) = parts.next() else { continue };
        let d = parts
            .next()
            .and_then(|d| d.trim_end_matches(['x', 'w']).parse::<f32>().ok())
            .unwrap_or(1.0);
        if best.as_ref().map_or(true, |(b, _)| d > *b) {
            best = Some((d, url.to_string()));
        }
    }
    best.map(|(_, url)| Pick { url, density: 1.0 }).or_else(|| img.src.map(|s| Pick { url: s.to_string(), density: 1.0 }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENVS: [(&str, Env); 4] = [
        ("dpr1_vw500", Env { viewport_w: 500.0, viewport_h: 900.0, dpr: 1.0 }),
        ("dpr1_vw784", Env { viewport_w: 784.0, viewport_h: 900.0, dpr: 1.0 }),
        ("dpr2_vw492", Env { viewport_w: 492.0, viewport_h: 900.0, dpr: 2.0 }),
        ("dpr2_vw784", Env { viewport_w: 784.0, viewport_h: 900.0, dpr: 2.0 }),
    ];

    /// (case, image, picture sources, expected per environment: (url, decoded px width of that
    /// url, natural width Chrome reported)). Chrome 157 on Windows, `--force-device-scale-factor`
    /// and `--window-size`, via `img.currentSrc` / `img.naturalWidth` (see the matrix page).
    type Expected = [(&'static str, f32, f32); 4];

    fn check(name: &str, img: SourceAttrs<'_>, sources: Vec<SourceAttrs<'_>>, expected: Expected) {
        for ((env_name, env), (url, px, natural)) in ENVS.iter().zip(expected.iter()) {
            let pick = select_source(&img, &sources, env)
                .unwrap_or_else(|| panic!("{name} @ {env_name}: nothing picked"));
            assert_eq!(pick.url, *url, "{name} @ {env_name}: which image");
            let got_natural = px / pick.density;
            assert!(
                (got_natural - natural).abs() < 1.0,
                "{name} @ {env_name}: natural width {got_natural} (density {}), Chrome {natural}",
                pick.density
            );
        }
    }

    fn img<'a>() -> SourceAttrs<'a> {
        SourceAttrs::default()
    }

    #[test]
    fn x_descriptors_pick_the_smallest_density_that_covers_the_dpr() {
        let a = SourceAttrs { srcset: Some("i100.png 1x, i200.png 2x"), ..img() };
        check("c1", a.clone(), vec![], [("i100.png", 100.0, 100.0), ("i100.png", 100.0, 100.0), ("i200.png", 200.0, 100.0), ("i200.png", 200.0, 100.0)]);
        let b = SourceAttrs { srcset: Some("i100.png 1x, i200.png 2x, i400.png 4x"), ..img() };
        check("c2", b, vec![], [("i100.png", 100.0, 100.0), ("i100.png", 100.0, 100.0), ("i200.png", 200.0, 100.0), ("i200.png", 200.0, 100.0)]);
        let only2x = SourceAttrs { srcset: Some("i200.png 2x"), ..img() };
        check("c3", only2x, vec![], [("i200.png", 200.0, 100.0); 4]);
    }

    #[test]
    fn src_is_the_1x_candidate_when_no_candidate_is_1x() {
        let c4 = SourceAttrs { src: Some("i50.png"), srcset: Some("i100.png 1.5x, i300.png 3x"), ..img() };
        check("c4", c4, vec![], [("i50.png", 50.0, 50.0), ("i50.png", 50.0, 50.0), ("i300.png", 300.0, 100.0), ("i300.png", 300.0, 100.0)]);
        let c14 = SourceAttrs { src: Some("i50.png"), srcset: Some(""), ..img() };
        check("c14", c14, vec![], [("i50.png", 50.0, 50.0); 4]);
    }

    #[test]
    fn a_candidate_with_no_descriptor_is_1x() {
        let c15 = SourceAttrs { srcset: Some("i100.png, i200.png 2x"), ..img() };
        check("c15", c15, vec![], [("i100.png", 100.0, 100.0), ("i100.png", 100.0, 100.0), ("i200.png", 200.0, 100.0), ("i200.png", 200.0, 100.0)]);
    }

    #[test]
    fn w_descriptors_with_sizes_pick_by_the_slot_width_and_the_natural_size_is_the_slot() {
        // No sizes means 100vw; the natural width is the slot (Chrome: 500, 784, 492, 784).
        let w = "i100.png 100w, i200.png 200w, i400.png 400w";
        check("c5", SourceAttrs { srcset: Some(w), sizes: Some("100vw"), ..img() }, vec![], [("i400.png", 400.0, 500.0), ("i400.png", 400.0, 784.0), ("i400.png", 400.0, 492.0), ("i400.png", 400.0, 784.0)]);
        check("c7", SourceAttrs { srcset: Some(w), ..img() }, vec![], [("i400.png", 400.0, 500.0), ("i400.png", 400.0, 784.0), ("i400.png", 400.0, 492.0), ("i400.png", 400.0, 784.0)]);
        check(
            "c6",
            SourceAttrs { srcset: Some(w), sizes: Some("(min-width: 600px) 300px, 100px"), ..img() },
            vec![],
            [("i100.png", 100.0, 100.0), ("i400.png", 400.0, 300.0), ("i200.png", 200.0, 100.0), ("i400.png", 400.0, 300.0)],
        );
        check("c9", SourceAttrs { srcset: Some(w), sizes: Some("150px"), ..img() }, vec![], [("i200.png", 200.0, 150.0), ("i200.png", 200.0, 150.0), ("i400.png", 400.0, 150.0), ("i400.png", 400.0, 150.0)]);
    }

    #[test]
    fn picture_sources_win_in_order_by_media_and_type() {
        let base = SourceAttrs { src: Some("i50.png"), ..img() };
        // c10: media min-width 600 first, then an unconditional source.
        let c10 = vec![
            SourceAttrs { media: Some("(min-width: 600px)"), srcset: Some("i400.png"), ..img() },
            SourceAttrs { srcset: Some("i100.png"), ..img() },
        ];
        check("c10", base.clone(), c10, [("i100.png", 100.0, 100.0), ("i400.png", 400.0, 400.0), ("i100.png", 100.0, 100.0), ("i400.png", 400.0, 400.0)]);
        // c11: an unsupported type is skipped; the img's own srcset decides.
        let c11 = vec![SourceAttrs { mime: Some("image/foo"), srcset: Some("i400.png"), ..img() }];
        let own = SourceAttrs { src: Some("i50.png"), srcset: Some("i100.png 1x, i200.png 2x"), ..img() };
        check("c11", own, c11, [("i100.png", 100.0, 100.0), ("i100.png", 100.0, 100.0), ("i200.png", 200.0, 100.0), ("i200.png", 200.0, 100.0)]);
        // c12: a supported type with x descriptors.
        let c12 = vec![SourceAttrs { mime: Some("image/png"), srcset: Some("i200.png 1x, i400.png 2x"), ..img() }];
        check("c12", base.clone(), c12, [("i200.png", 200.0, 200.0), ("i200.png", 200.0, 200.0), ("i400.png", 400.0, 200.0), ("i400.png", 400.0, 200.0)]);
        // c13: a source with w descriptors and its own sizes.
        let c13 = vec![SourceAttrs { srcset: Some("i100.png 100w, i300.png 300w"), sizes: Some("200px"), ..img() }];
        check("c13", base, c13, [("i300.png", 300.0, 200.0); 4]);
    }

    #[test]
    fn nothing_to_load_picks_nothing() {
        assert_eq!(select_source(&img(), &[], &ENVS[0].1), None);
    }
}

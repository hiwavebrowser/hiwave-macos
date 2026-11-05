//! Canvas 2D script surface (web_canvas.js): getContext, the context's
//! attributes as they read back, and the members a charting library calls.
//! Expected values from Chromium 143.0.7499.4 on the same scripts.

use super::*;

const PAGE: &str = "<!DOCTYPE html><html><body><canvas id=c></canvas><canvas id=d width=40 height=20px></canvas></body></html>";

fn bound_with(document: Rc<Document>) -> DomBindings {
    let bindings = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    bindings.set_document(document).unwrap();
    bindings
}

fn bound() -> DomBindings {
    bound_with(Rc::new(Document::parse_html(PAGE).unwrap()))
}

fn ev(bindings: &DomBindings, script: &str) -> String {
    match bindings.evaluate(script).unwrap() {
        JsValue::String(s) => s,
        other => panic!("{script} evaluated to {other:?}"),
    }
}

#[test]
fn canvas_size_reflects_its_attributes() {
    let b = bound();
    assert_eq!(
        ev(&b, "var c = document.getElementById('c'), d = document.getElementById('d'); \
                JSON.stringify([c.width, c.height, d.width, d.height])"),
        "[300,150,40,20]"
    );
    assert_eq!(
        ev(&b, "c.width = 64.7; c.height = '32'; JSON.stringify([c.width, c.getAttribute('width'), c.height])"),
        "[64,\"64\",32]"
    );
}

/// Chromium 143 (a canvas that has a 2D context answers null for any
/// other kind). Here there is no WebGL at all, so a fresh canvas answers
/// null for those kinds too.
#[test]
fn only_a_2d_context_is_handed_out() {
    let b = bound();
    assert_eq!(
        ev(&b, "var c = document.getElementById('c'); var x = c.getContext('2d'); \
                JSON.stringify([x instanceof CanvasRenderingContext2D, c.getContext('webgl'), c.getContext('webgl2'), \
                c.getContext('bitmaprenderer'), c.getContext('2d') === x])"),
        "[true,null,null,null,true]"
    );
    assert_eq!(
        ev(&b, "var d = document.getElementById('d'); JSON.stringify([d.getContext('webgl'), typeof CanvasRenderingContext2D, \
                (function () { try { new CanvasRenderingContext2D(); return 'made'; } catch (e) { return e.name; } })()])"),
        "[null,\"function\",\"TypeError\"]"
    );
}

#[test]
fn font_reads_back_normalized() {
    let b = bound();
    assert_eq!(
        ev(&b, "var x = document.getElementById('c').getContext('2d'); var out = []; \
                ['normal 12px Arial', 'italic 600 1em serif', '12px \"Helvetica Neue\", Arial', 'normal normal 400 14px sans-serif', 'bad', '700 12px Arial'] \
                .forEach(function (f) { x.font = f; out.push(x.font); }); JSON.stringify(out)"),
        r#"["12px Arial","italic 600 16px serif","12px \"Helvetica Neue\", Arial","14px sans-serif","14px sans-serif","bold 12px Arial"]"#
    );
}

#[test]
fn context_attributes_read_back_as_chromium_serializes_them() {
    let b = bound();
    assert_eq!(
        ev(&b, "var x = document.getElementById('c').getContext('2d'); var r = [x.fillStyle, x.strokeStyle, x.lineWidth, x.font, x.textAlign, x.textBaseline, x.globalAlpha, x.lineCap, x.lineJoin]; \
                x.fillStyle = 'rgba(6, 182, 212, 0.1)'; x.strokeStyle = 'RED'; x.lineWidth = 0; x.lineWidth = 3; x.font = 'bold 12px Arial'; \
                x.textAlign = 'middle'; x.textAlign = 'center'; x.globalAlpha = 2; x.globalAlpha = 0.5; x.fillStyle = 'nonsense'; \
                r = r.concat([x.fillStyle, x.strokeStyle, x.lineWidth, x.font, x.textAlign, x.globalAlpha]); JSON.stringify(r)"),
        "[\"#000000\",\"#000000\",1,\"10px sans-serif\",\"start\",\"alphabetic\",1,\"butt\",\"miter\",\
          \"rgba(6, 182, 212, 0.1)\",\"#ff0000\",3,\"bold 12px Arial\",\"center\",0.5]"
    );
}

#[test]
fn save_and_restore_bring_attributes_back() {
    let b = bound();
    assert_eq!(
        ev(&b, "var x = document.getElementById('c').getContext('2d'); x.fillStyle = '#123456'; x.setLineDash([1, 2, 3]); x.save(); \
                x.fillStyle = '#abcdef'; x.setLineDash([]); x.restore(); JSON.stringify([x.fillStyle, x.getLineDash()])"),
        "[\"#123456\",[1,2,3,1,2,3]]"
    );
}

#[test]
fn gradients_and_their_stops() {
    let b = bound();
    assert_eq!(
        ev(&b, "var x = document.getElementById('c').getContext('2d'); var g = x.createLinearGradient(0, 0, 100, 0); \
                g.addColorStop(0, 'red'); var errs = []; \
                try { g.addColorStop(2, 'red'); } catch (e) { errs.push(e.name); } \
                try { g.addColorStop(0.5, 'nope'); } catch (e) { errs.push(e.name); } \
                x.fillStyle = g; JSON.stringify([x.fillStyle === g, g instanceof CanvasGradient, errs])"),
        "[true,true,[\"IndexSizeError\",\"SyntaxError\"]]"
    );
}

/// Not a Chromium value: phase 1 defines the members it draws with and
/// leaves the rest out (web_interfaces.js policy: absent, not stubbed).
#[test]
fn drawing_members_are_there_and_the_rest_absent() {
    let b = bound();
    assert_eq!(
        ev(&b, "var x = document.getElementById('c').getContext('2d'); \
                var have = ['save','restore','scale','rotate','translate','transform','setTransform','resetTransform','fillRect','clearRect','strokeRect', \
                'beginPath','moveTo','lineTo','arc','arcTo','bezierCurveTo','quadraticCurveTo','rect','closePath','fill','stroke','clip', \
                'setLineDash','getLineDash','fillText','strokeText','measureText','createLinearGradient'].filter(function (m) { return typeof x[m] !== 'function'; }); \
                var absent = ['drawImage','createPattern','createRadialGradient','getImageData','putImageData','isPointInPath'].filter(function (m) { return m in x; }); \
                JSON.stringify([have, absent, typeof document.getElementById('c').toDataURL])"),
        "[[],[],\"undefined\"]"
    );
}

#[test]
fn measure_text_has_a_width() {
    let b = bound();
    assert_eq!(
        ev(&b, "var x = document.getElementById('c').getContext('2d'); x.font = '20px sans-serif'; var m = x.measureText('abcd'); \
                JSON.stringify([m instanceof TextMetrics, m.width > 0])"),
        "[true,true]"
    );
}

// STYLE1b (a): the inline style contract, as the pinned Chrome (CfT 148.0.7778.216) answers it.
// Each case gets a fresh <div> in the body and returns a JSON value; run() returns
// { name: value } for every case. The same script runs in Chrome (style1b_contract_chrome.mjs ->
// style1b_contract_cft148.json) and in RustKit's bindings (style1b_contract_tests.rs), which
// reports where they differ.
window.STYLE1B_CASES = [
  ['attr-verbatim', function (d) {
    d.setAttribute('style', '  COLOR : red ;; margin:1px 2px;/*c*/ ');
    return [d.getAttribute('style'), d.style.cssText, d.style.length];
  }],
  ['attr-duplicates', function (d) {
    d.setAttribute('style', 'color: red; color: blue; width: 1px');
    return [d.getAttribute('style'), d.style.cssText, d.style.length, d.style.color];
  }],
  ['attr-invalid-dropped', function (d) {
    d.setAttribute('style', 'color: notacolor; width: 2px; foo: bar');
    return [d.getAttribute('style'), d.style.cssText, d.style.length];
  }],
  ['set-one', function (d) {
    d.style.left = '3px';
    return [d.getAttribute('style'), d.style.cssText, d.style.length, d.style.item(0)];
  }],
  ['set-order-kept', function (d) {
    d.style.color = 'red'; d.style.width = '1px'; d.style.color = 'blue';
    return [d.getAttribute('style'), d.style.cssText];
  }],
  ['set-same-no-rewrite', function (d) {
    d.setAttribute('style', 'left:3px');
    d.style.left = '3px';
    return [d.getAttribute('style')];
  }],
  ['set-invalid-ignored', function (d) {
    d.style.color = 'red'; d.style.color = 'notacolor';
    return [d.getAttribute('style'), d.style.color];
  }],
  ['set-unitless-ignored', function (d) {
    d.style.left = '5';
    d.style.width = 0;
    return [d.getAttribute('style'), d.style.cssText];
  }],
  ['set-number', function (d) {
    d.style.opacity = 0.5; d.style.zIndex = 3;
    return [d.getAttribute('style'), d.style.opacity, d.style.zIndex];
  }],
  ['set-empty-removes', function (d) {
    d.style.color = 'red'; d.style.width = '1px'; d.style.color = '';
    return [d.getAttribute('style'), d.style.length];
  }],
  ['set-important', function (d) {
    d.style.setProperty('color', 'red', 'important');
    return [d.getAttribute('style'), d.style.getPropertyPriority('color'), d.style.cssText];
  }],
  ['remove-property', function (d) {
    d.style.cssText = 'color: red; width: 1px';
    var old = d.style.removeProperty('color');
    return [old, d.getAttribute('style'), d.style.length];
  }],
  ['shorthand-margin', function (d) {
    d.style.margin = '1px';
    var items = []; for (var i = 0; i < d.style.length; i++) items.push(d.style.item(i));
    return [d.getAttribute('style'), d.style.cssText, d.style.length, items, d.style.marginTop];
  }],
  ['shorthand-margin-then-longhand', function (d) {
    d.style.margin = '1px'; d.style.marginLeft = '2px';
    return [d.getAttribute('style'), d.style.margin];
  }],
  ['shorthand-background', function (d) {
    d.style.background = 'red';
    return [d.getAttribute('style'), d.style.length, d.style.backgroundColor];
  }],
  ['shorthand-in-attr', function (d) {
    d.setAttribute('style', 'margin: 1px 2px');
    return [d.getAttribute('style'), d.style.cssText, d.style.length, d.style.marginLeft];
  }],
  ['csstext-messy', function (d) {
    d.style.cssText = '  COLOR:RED;width : 1PX ; ';
    return [d.getAttribute('style'), d.style.cssText, d.style.color, d.style.width];
  }],
  ['custom-property', function (d) {
    d.style.setProperty('--x', '  a  b ');
    d.style.setProperty('--Y', 'c');
    return [d.getAttribute('style'), d.style.getPropertyValue('--x'), d.style.length];
  }],
  ['url-and-quotes', function (d) {
    d.style.backgroundImage = 'url("a b.png")';
    d.style.fontFamily = "'Open Sans', serif";
    return [d.getAttribute('style'), d.style.backgroundImage, d.style.fontFamily];
  }],
  ['color-normalised', function (d) {
    d.style.color = '#FF0000'; var a = d.style.color;
    d.style.color = 'RGB(1, 2, 3)'; var b = d.style.color;
    return [a, b, d.getAttribute('style')];
  }],
  ['transform', function (d) {
    d.style.transform = 'translate(10px, 20px) rotate(45deg)';
    return [d.getAttribute('style'), d.style.transform];
  }],
  ['camel-and-float', function (d) {
    d.style.backgroundColor = 'red'; d.style.cssFloat = 'left';
    return [d.getAttribute('style'), d.style['background-color'], d.style.cssFloat];
  }],
  ['unknown-property-ignored', function (d) {
    d.style.foo = 'bar'; d.style.setProperty('foo', 'bar');
    return [d.getAttribute('style'), d.style.length, typeof d.style.foo];
  }],
  ['mutation-records', function (d) {
    var log = [];
    var mo = new MutationObserver(function () {});
    mo.observe(d, { attributes: true, attributeOldValue: true });
    d.style.color = 'red'; d.style.color = 'red'; d.style.width = '1px'; d.style.color = 'blue';
    d.style.removeProperty('nothere');
    mo.takeRecords().forEach(function (r) { log.push([r.attributeName, r.oldValue]); });
    mo.disconnect();
    return log;
  }],
  ['remove-attribute-then-style', function (d) {
    d.style.color = 'red'; d.removeAttribute('style');
    return [d.getAttribute('style'), d.style.length, d.style.cssText];
  }]
];
window.STYLE1B_RUN = function () {
  var out = {};
  for (var i = 0; i < STYLE1B_CASES.length; i++) {
    var d = document.createElement('div');
    document.body.appendChild(d);
    try { out[STYLE1B_CASES[i][0]] = STYLE1B_CASES[i][1](d); }
    catch (e) { out[STYLE1B_CASES[i][0]] = 'threw ' + (e && e.name); }
    d.remove();
  }
  return out;
};

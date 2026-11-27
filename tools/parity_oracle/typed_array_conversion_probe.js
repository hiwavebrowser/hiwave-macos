(function () {
  var r = [];
  var i8 = new Int8Array([-56, -1, 5, -128, 127]);
  r.push('u8<-i8:' + Array.from(new Uint8Array(i8)).join(','));
  r.push('u16<-i8:' + Array.from(new Uint16Array(i8)).join(','));
  r.push('u32<-i8:' + Array.from(new Uint32Array(i8)).join(','));
  var u8 = new Uint8Array([200, 255, 128, 5]);
  r.push('i8<-u8:' + Array.from(new Int8Array(u8)).join(','));
  var f = new Float64Array([300.7, -1.5, 1e10, NaN, Infinity, -0.5, 2.5, 3.5, 254.5]);
  r.push('u8<-f64:' + Array.from(new Uint8Array(f)).join(','));
  r.push('i8<-f64:' + Array.from(new Int8Array(f)).join(','));
  r.push('i32<-f64:' + Array.from(new Int32Array(f)).join(','));
  r.push('u8c<-f64:' + Array.from(new Uint8ClampedArray(f)).join(','));
  var i32 = new Int32Array([-1, 70000, -70000]);
  r.push('i16<-i32:' + Array.from(new Int16Array(i32)).join(','));
  r.push('u16<-i32:' + Array.from(new Uint16Array(i32)).join(','));
  return r.join('\n');
})()

#!/usr/bin/env python3
"""Measure the advance of a string under the macOS system font at several CSS
weights, via Core Text directly.

Purpose: prove (or kill) the hypothesis that RustKit's binary system-font weight
gate (weight>=600 ? emphasized : regular) is what makes `about`'s .tagline wrap.
Chrome/Skia map CSS weight -> kCTFontWeightTrait on a 9-point table; RustKit
collapses 100..500 to Regular.
"""
import ctypes
from ctypes import c_void_p, c_double, c_uint32, c_long, c_int32, byref

ct = ctypes.CDLL('/System/Library/Frameworks/CoreText.framework/CoreText')
cf = ctypes.CDLL('/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation')

for fn, res, args in [
    ('CTFontCreateUIFontForLanguage', c_void_p, [c_uint32, c_double, c_void_p]),
    ('CTFontCopyFontDescriptor', c_void_p, [c_void_p]),
    ('CTFontDescriptorCreateCopyWithAttributes', c_void_p, [c_void_p, c_void_p]),
    ('CTFontCreateWithFontDescriptor', c_void_p, [c_void_p, c_double, c_void_p]),
    ('CTFontCopyPostScriptName', c_void_p, [c_void_p]),
    ('CTLineCreateWithAttributedString', c_void_p, [c_void_p]),
    ('CTLineGetTypographicBounds', c_double, [c_void_p, c_void_p, c_void_p, c_void_p]),
]:
    f = getattr(ct, fn); f.restype = res; f.argtypes = args

for fn, res, args in [
    ('CFStringCreateWithCString', c_void_p, [c_void_p, ctypes.c_char_p, c_uint32]),
    ('CFStringGetCString', ctypes.c_bool, [c_void_p, ctypes.c_char_p, c_long, c_uint32]),
    ('CFNumberCreate', c_void_p, [c_void_p, c_int32, c_void_p]),
    ('CFDictionaryCreate', c_void_p, [c_void_p, c_void_p, c_void_p, c_long, c_void_p, c_void_p]),
    ('CFAttributedStringCreate', c_void_p, [c_void_p, c_void_p, c_void_p]),
    ('CFRelease', None, [c_void_p]),
]:
    f = getattr(cf, fn); f.restype = res; f.argtypes = args

kCFStringEncodingUTF8 = 0x08000100
kCTFontSystemFontType = 2
kCFNumberDoubleType = 13
kCFTypeDictionaryKeyCallBacks = c_void_p.in_dll(cf, 'kCFTypeDictionaryKeyCallBacks')
kCFTypeDictionaryValueCallBacks = c_void_p.in_dll(cf, 'kCFTypeDictionaryValueCallBacks')
kCTFontTraitsAttribute = c_void_p.in_dll(ct, 'kCTFontTraitsAttribute')
kCTFontWeightTrait = c_void_p.in_dll(ct, 'kCTFontWeightTrait')
kCTFontAttributeName = c_void_p.in_dll(ct, 'kCTFontAttributeName')


def cfstr(s):
    return cf.CFStringCreateWithCString(None, s.encode('utf-8'), kCFStringEncodingUTF8)


def pystr(cfs):
    buf = ctypes.create_string_buffer(512)
    cf.CFStringGetCString(cfs, buf, 512, kCFStringEncodingUTF8)
    return buf.value.decode('utf-8')


def cfdict(pairs):
    n = len(pairs)
    keys = (c_void_p * n)(*[k for k, _ in pairs])
    vals = (c_void_p * n)(*[v for _, v in pairs])
    return cf.CFDictionaryCreate(None, keys, vals, n,
                                 byref(kCFTypeDictionaryKeyCallBacks),
                                 byref(kCFTypeDictionaryValueCallBacks))


def system_font(size, ct_weight):
    """System font at `size`, with kCTFontWeightTrait forced to ct_weight."""
    base = ct.CTFontCreateUIFontForLanguage(kCTFontSystemFontType, size, None)
    if ct_weight is None:
        return base
    d = ct.CTFontCopyFontDescriptor(base)
    num = cf.CFNumberCreate(None, kCFNumberDoubleType, byref(c_double(ct_weight)))
    traits = cfdict([(kCTFontWeightTrait, num)])
    attrs = cfdict([(kCTFontTraitsAttribute, traits)])
    d2 = ct.CTFontDescriptorCreateCopyWithAttributes(d, attrs)
    return ct.CTFontCreateWithFontDescriptor(d2, size, None)


def advance(font, text):
    s = cfstr(text)
    attrs = cfdict([(kCTFontAttributeName, font)])
    astr = cf.CFAttributedStringCreate(None, s, attrs)
    line = ct.CTLineCreateWithAttributedString(astr)
    asc, desc, lead = c_double(), c_double(), c_double()
    w = ct.CTLineGetTypographicBounds(line, byref(asc), byref(desc), byref(lead))
    return w, asc.value, desc.value, lead.value


# Skia's SkFontStyle <-> kCTFontWeightTrait table (SkTypeface_mac / SkFontHost_mac).
SKIA_WEIGHT_TABLE = [
    (100, -0.80), (200, -0.60), (300, -0.40), (400, 0.00), (500, 0.23),
    (600, 0.30), (700, 0.40), (800, 0.56), (900, 0.62),
]

TAGLINE = "A browser that doesn't track you. Doesn't sell you. Doesn't belong to Google."

if __name__ == '__main__':
    size = 20.0  # about.html .tagline: font-size: 1.25rem
    print(f'string: {TAGLINE!r}')
    print(f'size:   {size}px   (Chrome fits this in a 672px block, h=23, 1 line)')
    print()
    print(f'{"css_w":>6} {"ct_trait":>9}  {"advance":>10}  {"psname":<34} {"fits 672?"}')
    for css_w, trait in SKIA_WEIGHT_TABLE:
        f = system_font(size, trait)
        w, asc, desc, lead = advance(f, TAGLINE)
        ps = pystr(ct.CTFontCopyPostScriptName(f))
        print(f'{css_w:>6} {trait:>9.2f}  {w:>10.3f}  {ps:<34} {"YES" if w <= 672.0 else "no"}')
    print()
    f = system_font(size, None)
    w, *_ = advance(f, TAGLINE)
    ps = pystr(ct.CTFontCopyPostScriptName(f))
    print(f'{"UI/reg":>6} {"(none)":>9}  {w:>10.3f}  {ps:<34} '
          f'<-- what RustKit uses for EVERY weight < 600')

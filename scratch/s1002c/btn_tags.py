"""Print the <button ...> open tags of a live page whose following text contains a needle (first 3).
Page text is untrusted data: only printed as measurements.
usage: btn_tags.py <url> <needle>"""
import subprocess, sys
UA = ('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) '
      'Version/17.0 Safari/605.1.15')
html = subprocess.run(['curl', '-sL', '--compressed', '--max-time', '25', '-A', UA, sys.argv[1]],
                      capture_output=True, text=True, errors='replace').stdout
needle = sys.argv[2].lower()
i = n = 0
while n < 3:
    a = html.find('<button', i)
    if a < 0:
        break
    b = html.find('>', a)
    after = html[b + 1:b + 400].lower()
    if needle in after:
        print(html[a:b + 1][:500])
        n += 1
    i = b

# python3 reddit_707_serve.py <port>: http.server for this folder; a .js request with ?delay=<ms> is held that long (a slow CDN).
import http.server, sys, time, urllib.parse
class Slow(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        q = urllib.parse.parse_qs(urllib.parse.urlparse(self.path).query)
        if urllib.parse.urlparse(self.path).path.endswith('.js'): time.sleep(int(q.get('delay', ['0'])[0]) / 1000)
        return super().do_GET()
http.server.ThreadingHTTPServer(('127.0.0.1', int(sys.argv[1])), Slow).serve_forever()

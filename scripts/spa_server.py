import http.server
import os
import sys

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 4190
DIR = sys.argv[2] if len(sys.argv) > 2 else "."
BASE_PATH = sys.argv[3].rstrip("/") if len(sys.argv) > 3 else ""

CACHE_IMMUTABLE = {".wasm", ".js", ".css", ".png", ".jpg", ".svg", ".woff2"}

class SPAHandler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=DIR, **kwargs)

    def _strip_base(self):
        if BASE_PATH and self.path.startswith(BASE_PATH + "/"):
            self.path = self.path[len(BASE_PATH):]

    def send_head(self):
        self._strip_base()
        path = self.translate_path(self.path)
        if not os.path.exists(path) or os.path.isdir(path):
            self.path = "/"
            return super().send_head()
        if self._try_serve_brotli(path):
            return None
        return super().send_head()

    def _try_serve_brotli(self, path):
        accept = self.headers.get("Accept-Encoding", "")
        if "br" not in accept:
            return False
        br_path = path + ".br"
        if not os.path.exists(br_path):
            return False
        try:
            f = open(br_path, "rb")
            fs = os.fstat(f.fileno())
            ctype = self.guess_type(path)
            self.send_response(200)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Encoding", "br")
            self.send_header("Content-Length", str(fs.st_size))
            ext = os.path.splitext(path)[1].lower()
            if ext in CACHE_IMMUTABLE:
                self.send_header("Cache-Control", "public, max-age=86400, immutable")
            self.end_headers()
            self.copyfile(f, self.wfile)
            f.close()
            return True
        except OSError:
            return False

    def log_message(self, format, *args):
        if len(args) >= 2 and args[0].startswith("Broken pipe"):
            return
        super().log_message(format, *args)

class SilentServer(http.server.HTTPServer):
    def handle_error(self, request, client_address):
        e_type, e_val, _ = sys.exc_info()
        if isinstance(e_val, (BrokenPipeError, ConnectionResetError)):
            return
        super().handle_error(request, client_address)

with SilentServer(("", PORT), SPAHandler) as httpd:
    print(f"SPA server on port {PORT}, directory={DIR}, base={BASE_PATH or '/'}")
    httpd.serve_forever()

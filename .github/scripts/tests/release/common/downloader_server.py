"""Loopback HTTP assets shared by native Release downloader tests."""

import hashlib
import http.server
import stat
import threading
import zipfile


class ReleaseHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        self.server.requests += 1
        if getattr(self.server, "interrupt_archive", False) and self.path.endswith(self.server.archive_name):
            self.send_response(200)
            self.send_header("Content-Length", "9999")
            self.end_headers()
            self.wfile.write(b"partial")
            self.close_connection = True
            return
        super().do_GET()


def start_server(root, archive_name, add_cleanup):
    handler = lambda *args, **kwargs: ReleaseHandler(*args, directory=str(root), **kwargs)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    server.archive_name = archive_name
    server.requests = 0
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    add_cleanup(server.server_close)
    add_cleanup(thread.join)
    add_cleanup(server.shutdown)
    return server


def write_assets(assets, archive_name, members, file_mode):
    archive_path = assets / archive_name
    with zipfile.ZipFile(archive_path, "w") as archive:
        for name, content in members.items():
            info = zipfile.ZipInfo(name)
            info.create_system = 3
            info.external_attr = (stat.S_IFREG | file_mode) << 16
            archive.writestr(info, content)
    write_checksum(assets, archive_name)


def write_checksum(assets, archive_name):
    digest = hashlib.sha256((assets / archive_name).read_bytes()).hexdigest()
    (assets / "SHA256SUMS.txt").write_text(f"{digest}  {archive_name}\n", encoding="ascii")

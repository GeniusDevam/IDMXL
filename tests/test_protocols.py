"""Private CA and ALPN HTTP/2 fixtures. No OS trust changes or TLS bypass."""
import datetime
import hashlib
import http.server
import json
import os
from pathlib import Path
import socketserver
import ssl
import subprocess
import tempfile
import threading
import unittest
import uuid
from test_transfers import DATA, DIGEST, EXE, ROOT

ENABLED = bool(os.environ.get("IDMXL_PROTOCOL_TESTS"))

@unittest.skipUnless(ENABLED, "Run scripts/test.ps1 -Protocols with tests/requirements.txt")
class ProtocolTests(unittest.TestCase):
    def setUp(self):
        from cryptography import x509
        from cryptography.hazmat.primitives import hashes, serialization
        from cryptography.hazmat.primitives.asymmetric import rsa
        from cryptography.x509.oid import NameOID
        scratch=ROOT / ".test-output"; scratch.mkdir(exist_ok=True)
        self.temp=tempfile.TemporaryDirectory(dir=scratch,prefix="protocol-")
        self.directory=Path(self.temp.name); self.state=self.directory / "state"
        self.servers=[]
        class CrlHandler(http.server.BaseHTTPRequestHandler):
            def log_message(self,*_): pass
            def do_GET(inner):
                body=inner.server.crl
                inner.send_response(200);inner.send_header("Content-Type","application/pkix-crl");inner.send_header("Content-Length",str(len(body)));inner.end_headers();inner.wfile.write(body)
        crl_server=http.server.ThreadingHTTPServer(("127.0.0.1",0),CrlHandler)
        crl_server.crl=b"";self.servers.append(crl_server)
        threading.Thread(target=crl_server.serve_forever,daemon=True).start()
        self.crl_dp=x509.CRLDistributionPoints([x509.DistributionPoint(full_name=[x509.UniformResourceIdentifier(f"http://127.0.0.1:{crl_server.server_port}/fixture.crl")],relative_name=None,reasons=None,crl_issuer=None)])
        self.key=rsa.generate_private_key(public_exponent=65537,key_size=2048)
        name=x509.Name([x509.NameAttribute(NameOID.COMMON_NAME,"IDMXL ephemeral test CA "+str(uuid.uuid4()))])
        now=datetime.datetime.now(datetime.timezone.utc)
        self.ca=x509.CertificateBuilder().subject_name(name).issuer_name(name).public_key(self.key.public_key()).serial_number(x509.random_serial_number()).not_valid_before(now-datetime.timedelta(days=1)).not_valid_after(now+datetime.timedelta(days=1)).add_extension(x509.BasicConstraints(ca=True,path_length=0),critical=True).add_extension(x509.SubjectKeyIdentifier.from_public_key(self.key.public_key()),critical=False).add_extension(self.crl_dp,critical=False).add_extension(x509.KeyUsage(digital_signature=True,content_commitment=False,key_encipherment=False,data_encipherment=False,key_agreement=False,key_cert_sign=True,crl_sign=True,encipher_only=False,decipher_only=False),critical=True).sign(self.key,hashes.SHA256())
        crl=x509.CertificateRevocationListBuilder().issuer_name(name).last_update(now-datetime.timedelta(hours=1)).next_update(now+datetime.timedelta(days=1)).sign(self.key,hashes.SHA256())
        crl_server.crl=crl.public_bytes(serialization.Encoding.DER)
        self.ca_path=self.directory / "ca.pem"; self.ca_path.write_bytes(self.ca.public_bytes(serialization.Encoding.PEM))

    def tearDown(self):
        for server in self.servers: server.shutdown(); server.server_close()
        self.temp.cleanup()

    def server(self, hostname="localhost", expired=False):
        import h2.connection, h2.config, h2.events
        from cryptography import x509
        from cryptography.hazmat.primitives import hashes, serialization
        from cryptography.hazmat.primitives.asymmetric import rsa
        from cryptography.x509.oid import NameOID
        key=rsa.generate_private_key(public_exponent=65537,key_size=2048)
        name=x509.Name([x509.NameAttribute(NameOID.COMMON_NAME,hostname)])
        now=datetime.datetime.now(datetime.timezone.utc)
        cert=x509.CertificateBuilder().subject_name(name).issuer_name(self.ca.subject).public_key(key.public_key()).serial_number(x509.random_serial_number()).not_valid_before(now-datetime.timedelta(days=3)).not_valid_after(now+datetime.timedelta(days=-1 if expired else 1)).add_extension(x509.SubjectAlternativeName([x509.DNSName(hostname)]),critical=False).add_extension(x509.BasicConstraints(ca=False,path_length=None),critical=True).add_extension(x509.AuthorityKeyIdentifier.from_issuer_public_key(self.key.public_key()),critical=False).add_extension(self.crl_dp,critical=False).sign(self.key,hashes.SHA256())
        suffix=str(len(self.servers)); cert_path=self.directory / (suffix+"cert.pem"); key_path=self.directory / (suffix+"key.pem")
        cert_path.write_bytes(cert.public_bytes(serialization.Encoding.PEM))
        key_path.write_bytes(key.private_bytes(serialization.Encoding.PEM,serialization.PrivateFormat.PKCS8,serialization.NoEncryption()))
        context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER); context.load_cert_chain(cert_path,key_path); context.set_alpn_protocols(["h2"])
        class Handler(socketserver.BaseRequestHandler):
            def handle(inner):
                try:
                    with context.wrap_socket(inner.request,server_side=True) as stream:
                        stream.settimeout(5)
                        if stream.selected_alpn_protocol() != "h2": return
                        inner.server.negotiated.append("h2")
                        conn=h2.connection.H2Connection(config=h2.config.H2Configuration(client_side=False,header_encoding="utf-8"))
                        conn.initiate_connection(); stream.sendall(conn.data_to_send()); pending={}
                        while True:
                            incoming=stream.recv(65536)
                            if not incoming: return
                            for event in conn.receive_data(incoming):
                                if isinstance(event,h2.events.RequestReceived):
                                    headers=dict(event.headers); start=0; end=len(DATA)-1; status="200"
                                    if "range" in headers:
                                        start,end=map(int,headers["range"].removeprefix("bytes=").split("-")); status="206"
                                    end=min(end,len(DATA)-1)
                                    response=[(":status",status),("content-length",str(end-start+1)),("etag",'"h2-v1"')]
                                    if status=="206": response.append(("content-range",f"bytes {start}-{end}/{len(DATA)}"))
                                    conn.send_headers(event.stream_id,response)
                                    pending[event.stream_id]=[start,end+1]
                                elif isinstance(event,h2.events.StreamReset): pending.pop(event.stream_id,None)
                            for sid,(offset,end) in list(pending.items()):
                                while offset<end:
                                    count=min(conn.local_flow_control_window(sid),conn.max_outbound_frame_size,end-offset)
                                    if count<=0: break
                                    conn.send_data(sid,DATA[offset:offset+count],end_stream=offset+count==end); offset+=count
                                if offset==end: pending.pop(sid)
                                else: pending[sid]=[offset,end]
                            outgoing=conn.data_to_send()
                            if outgoing: stream.sendall(outgoing)
                except (ssl.SSLError,OSError): pass
        class Server(socketserver.ThreadingTCPServer):
            allow_reuse_address=True; daemon_threads=True; block_on_close=True
        server=Server(("127.0.0.1",0),Handler); server.negotiated=[]
        thread=threading.Thread(target=server.serve_forever,daemon=True); thread.start()
        self.servers.append(server)
        return server

    def download(self, server, trust=False):
        url=f"https://localhost:{server.server_address[1]}/file"
        output=self.directory / (str(len(self.servers))+"result.bin")
        result=subprocess.run([str(EXE),"--state-dir",str(self.state),"add","--url-stdin","--replayable","--output",str(output),"--sha256",DIGEST],input=url,text=True,capture_output=True,timeout=10)
        self.assertEqual(result.returncode,0,result.stderr)
        job=json.loads(result.stdout)
        env=dict(os.environ)
        binary=str(EXE)
        if trust:
            binary=os.environ["IDMXL_FAULT_EXE"]; env["IDMXL_TEST_CA"]=str(self.ca_path)
        run=subprocess.run([binary,"--state-dir",str(self.state),"run",job["id"]],capture_output=True,text=True,env=env,timeout=20)
        return run,output

    def test_verified_tls_negotiates_h2_and_transfers_ranges(self):
        server=self.server(); result,output=self.download(server,trust=True)
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual(json.loads(result.stdout)["mode"],"ranged")
        self.assertEqual(hashlib.sha256(output.read_bytes()).hexdigest(),DIGEST)
        self.assertTrue(server.negotiated)

    def test_untrusted_certificate_never_publishes(self):
        result,output=self.download(self.server())
        self.assertNotEqual(result.returncode,0)
        self.assertIn("tls_verification_failed",result.stderr)
        self.assertFalse(output.exists())

    def test_hostname_mismatch_never_publishes(self):
        result,output=self.download(self.server(hostname="wrong.test"),trust=True)
        self.assertNotEqual(result.returncode,0)
        self.assertIn("tls_verification_failed",result.stderr)
        self.assertFalse(output.exists())

    def test_expired_certificate_never_publishes(self):
        result,output=self.download(self.server(expired=True),trust=True)
        self.assertNotEqual(result.returncode,0)
        self.assertIn("tls_verification_failed",result.stderr)
        self.assertFalse(output.exists())

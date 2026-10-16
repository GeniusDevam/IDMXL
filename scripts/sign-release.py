"""Operator-only signing of the exact UTF-8 release payload. Never used by the shipped app."""
import argparse, base64, getpass, hashlib, json
from pathlib import Path
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives import serialization

def main():
    p=argparse.ArgumentParser();p.add_argument('--generate-key',action='store_true');p.add_argument('--key',type=Path,required=True);p.add_argument('--public-file',type=Path);p.add_argument('--manifest',type=Path);p.add_argument('--installer',type=Path);p.add_argument('--output',type=Path);a=p.parse_args()
    password=getpass.getpass('Signing key password: ').encode()
    if a.generate_key:
        if not password or not a.public_file:p.error('A password and --public-file are required.')
        if a.key.exists() or a.public_file.exists():p.error('Key and public-key output paths must both be new.')
        key=Ed25519PrivateKey.generate()
        with a.key.open('xb') as stream:stream.write(key.private_bytes(serialization.Encoding.PEM,serialization.PrivateFormat.PKCS8,serialization.BestAvailableEncryption(password)))
        with a.public_file.open('x',encoding='ascii') as stream:stream.write(base64.b64encode(key.public_key().public_bytes(serialization.Encoding.Raw,serialization.PublicFormat.Raw)).decode())
        print('Created encrypted signing key and public key. Keep the private key outside the repository and retain an offline recovery copy.');return
    if not (a.manifest and a.installer and a.output):p.error('--manifest, --installer and --output are required.')
    key=serialization.load_pem_private_key(a.key.read_bytes(),password=password or None)
    if not isinstance(key,Ed25519PrivateKey):p.error('An Ed25519 key is required.')
    manifest=json.loads(a.manifest.read_text(encoding='utf-8-sig'));digest=hashlib.file_digest(a.installer.open('rb'),'sha256').hexdigest()
    if manifest['sha256'].lower()!=digest or manifest['size']!=a.installer.stat().st_size:p.error('The manifest does not match the signed installer.')
    payload=json.dumps(manifest,ensure_ascii=False,separators=(',',':')).encode()
    envelope={'payload':base64.b64encode(payload).decode(),'signature':base64.b64encode(key.sign(payload)).decode()}
    with a.output.open('x',encoding='utf-8') as stream:json.dump(envelope,stream,separators=(',',':'))
    print('Created signed release envelope. Publish this file only with its matching installer.')
if __name__=='__main__':main()

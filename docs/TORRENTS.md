# Torrent downloads

IDMXL 0.2.0 bundles libtorrent 2.1.1 in its own background worker. A separate
Python installation is not needed to run the packaged app.

## Add a torrent

Choose **Add torrent** and paste a magnet link or HTTP(S) torrent URL, or choose
one or more local `.torrent` files. You can also drop torrent files onto IDMXL,
paste magnet links outside a text field, or send a torrent link from its Chrome
or Edge extension. Torrent-file browser captures are exempt from the 1 MB
ordinary-file threshold.

IDMXL retrieves the metadata and opens a review. Choose the destination folder,
skip unwanted files or set priorities, then choose **Start download**. Magnet
metadata discovery does not download file payloads before confirmation. Cancel
or close the review to remove an unconfirmed torrent.

The installer registers IDMXL as an available Windows handler for `.torrent`
files and `magnet:` links. Existing default applications are preserved. Use
Windows **Open with** or **Default apps** if you want IDMXL to handle them outside
the browser extension.

## Control downloads

The torrent list shows progress, download/upload speed, connected peers and
seeds. **Pause**, **Resume**, **Cancel** and **Retry** control the native worker.
Closing the desktop interface does not stop it. Details provide file priorities,
verification, tracker announcements, tracker/web-seed additions, peer information,
magnet copying, torrent export and file relocation.

**Remove from list** preserves files. **Delete files** requires a second
confirmation and removes that torrent's files, including partial files. Cancel
or closing the confirmation keeps the files. The main bulk controls include
torrents; torrent-specific bulk controls are also available.

Existing destination files are rejected unless you explicitly select **Verify
and reuse existing files**. Do that only for files belonging to this torrent.
Moves reject destination collisions. All paths are validated, torrent symlinks
are rejected, and skipped files are not selected for download.

## Speed, sharing and recovery

The engine supports v1, v2 and hybrid torrents, HTTP/HTTPS/UDP trackers, DHT,
peer exchange, local discovery, IPv4/IPv6, TCP/uTP, protocol encryption and HTTP
web seeds. Private-torrent discovery rules are enforced by libtorrent. Normal
piece selection uses libtorrent's scheduling; sequential mode is optional.

Defaults are three active torrent downloads, five active seeds, 500 total peer
connections, 100 per torrent, unlimited download speed and 524,288 bytes/s
upload. A new torrent stops seeding at ratio 1.0 by default. These settings are
adjustable. Torrent limits and queues are separate from HTTP/media queue limits.
Faster settings cannot compensate for missing seeds or a constrained swarm.

Verified piece state is checkpointed every ten seconds and on important control
changes. SQLite uses WAL and full synchronization. Metadata, tracker credentials
and resume records are protected with Windows DPAPI. Unclean restarts trigger a
piece recheck; damaged pieces are downloaded again. Completion waits for disk
flush rather than relying on a rounded 100% progress value.

**Create torrent** can hash a local file or folder into v1, v2 or hybrid format,
with trackers and an optional private flag. Creating a torrent does not publish
or upload the source files automatically.

## Boundaries

The first release focuses on conventional BitTorrent transfers. It does not
claim every µTorrent feature: RSS automation, remote web administration, I2P/SAM
configuration and a public torrent search service are not included. Swarm speed
and availability depend on other peers, router/firewall reachability and the
connection. It does not bypass tracker access controls or browser security
decisions.

Protocol and engine references: [libtorrent features](https://www.libtorrent.org/features.html),
[fast resume and storage controls](https://www.libtorrent.org/reference-Torrent_Handle.html),
[settings](https://www.libtorrent.org/reference-Settings.html).

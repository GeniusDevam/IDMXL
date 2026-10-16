//! Deliberately bounded clear VOD parsers. Unknown timeline/protection features fail closed.
use crate::{platform, small_http};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
const MAX_PIECES: usize = 10_000;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Piece {
    pub url: String,
    pub range: Option<(u64, u64)>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track {
    pub role: String,
    pub pieces: Vec<Piece>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration: Option<f64>,
}
#[derive(Clone, Debug)]
pub struct Choice {
    pub id: String,
    pub label: String,
    pub role: String,
    pub url: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub group: Option<String>,
    pub track: Option<Track>,
}
#[derive(Serialize)]
pub struct ChoiceView {
    pub id: String,
    pub label: String,
    pub role: String,
}
#[derive(Serialize)]
pub struct Inspection {
    pub kind: String,
    pub choices: Vec<ChoiceView>,
}
pub struct Discovery {
    pub kind: String,
    pub choices: Vec<Choice>,
}
impl Discovery {
    pub fn view(&self) -> Inspection {
        Inspection {
            kind: self.kind.clone(),
            choices: self
                .choices
                .iter()
                .map(|c| ChoiceView {
                    id: c.id.clone(),
                    label: c
                        .label
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(200)
                        .collect(),
                    role: c.role.clone(),
                })
                .collect(),
        }
    }
}
fn identity(url: &str, label: &str) -> String {
    format!("{:x}", Sha256::digest(format!("{url}\n{label}")))
}
fn joined(base: &str, value: &str) -> Result<String> {
    let base = platform::checked_url(base)?;
    let next = base
        .join(value)
        .map_err(|_| anyhow::anyhow!("invalid_media_url"))?;
    platform::checked_url(next.as_str())?;
    if base.scheme() == "https" && next.scheme() != "https" {
        bail!("insecure_redirect");
    }
    Ok(next.into())
}
fn choice(
    url: String,
    label: String,
    role: &str,
    width: Option<u32>,
    height: Option<u32>,
    group: Option<String>,
    track: Option<Track>,
) -> Choice {
    Choice {
        id: identity(&url, &label),
        url,
        label,
        role: role.into(),
        width,
        height,
        group,
        track,
    }
}
pub fn discover(url: &str, public: bool) -> Result<Discovery> {
    let source = platform::checked_url(url)?;
    let path = source.path().to_ascii_lowercase();
    if path.ends_with(".m3u8") || path.ends_with(".mpd") {
        let (resolved, bytes) = small_http::fetch(source.as_str(), 2 * 1024 * 1024, public)?;
        let text = std::str::from_utf8(&bytes).context("invalid_media_manifest")?;
        if path.ends_with(".m3u8") {
            hls(&resolved, text)
        } else {
            dash(&resolved, text)
        }
    } else {
        Ok(Discovery {
            kind: "direct".into(),
            choices: vec![choice(
                source.into(),
                "Original direct file".into(),
                "muxed",
                None,
                None,
                None,
                None,
            )],
        })
    }
}
fn attrs(text: &str) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let bytes = text.as_bytes();
    let mut start = 0;
    let mut quoted = false;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'"' {
            quoted = !quoted;
        }
        if b == b',' && !quoted {
            attribute(&text[start..i], &mut out)?;
            start = i + 1;
        }
    }
    if quoted {
        bail!("invalid_media_manifest");
    }
    attribute(&text[start..], &mut out)?;
    Ok(out)
}
fn attribute(text: &str, out: &mut BTreeMap<String, String>) -> Result<()> {
    let (key, value) = text.split_once('=').context("invalid_media_manifest")?;
    if out
        .insert(key.trim().into(), value.trim().trim_matches('"').into())
        .is_some()
    {
        bail!("invalid_media_manifest");
    }
    Ok(())
}
fn byte_range(text: &str, previous: Option<(String, u64)>, url: &str) -> Result<(u64, u64)> {
    let (length, offset) = text
        .split_once('@')
        .map_or((text, None), |(l, o)| (l, Some(o)));
    let length = length.parse::<u64>().context("invalid_media_range")?;
    let offset = match offset {
        Some(o) => o.parse().context("invalid_media_range")?,
        None => {
            previous
                .filter(|p| p.0 == url)
                .context("invalid_media_range")?
                .1
        }
    };
    if length == 0 || offset.checked_add(length).is_none() {
        bail!("invalid_media_range");
    }
    Ok((offset, length))
}
pub fn hls(base: &str, text: &str) -> Result<Discovery> {
    let lines: Vec<_> = text
        .trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if lines.first() != Some(&"#EXTM3U") {
        bail!("invalid_media_manifest");
    }
    if lines.iter().any(|l| {
        l.starts_with("#EXT-X-KEY:")
            || l.starts_with("#EXT-X-SESSION-KEY:")
            || l.starts_with("#EXT-X-DISCONTINUITY")
            || l.starts_with("#EXT-X-PART")
            || l.starts_with("#EXT-X-SKIP")
            || l.starts_with("#EXT-X-GAP")
            || l.starts_with("#EXT-X-DEFINE")
    }) {
        bail!("unsupported_media_protection_or_timeline");
    }
    let master = lines.iter().any(|l| l.starts_with("#EXT-X-STREAM-INF:"));
    if master {
        let mut choices = Vec::new();
        let mut pending = None;
        for line in &lines {
            if let Some(value) = line.strip_prefix("#EXT-X-STREAM-INF:") {
                if pending.is_some() {
                    bail!("invalid_media_manifest");
                }
                pending = Some(attrs(value)?);
            } else if let Some(value) = line.strip_prefix("#EXT-X-MEDIA:") {
                let a = attrs(value)?;
                if a.get("TYPE").is_some_and(|s| s == "AUDIO")
                    && let Some(uri) = a.get("URI")
                {
                    let url = joined(base, uri)?;
                    let label = format!(
                        "Audio: {} ({})",
                        a.get("NAME").map_or("Unnamed", String::as_str),
                        a.get("LANGUAGE")
                            .map_or("unspecified language", String::as_str)
                    );
                    choices.push(choice(
                        url,
                        label,
                        "audio",
                        None,
                        None,
                        a.get("GROUP-ID").cloned(),
                        None,
                    ));
                }
            } else if !line.starts_with('#') {
                let a = pending.take().context("invalid_media_manifest")?;
                let dimensions = a
                    .get("RESOLUTION")
                    .and_then(|s| s.split_once('x'))
                    .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)));
                let label = format!(
                    "{} · {} bit/s · {}",
                    a.get("RESOLUTION")
                        .map_or("Original dimensions", String::as_str),
                    a.get("BANDWIDTH").map_or("unknown", String::as_str),
                    a.get("CODECS").map_or("original codecs", String::as_str)
                );
                choices.push(choice(
                    joined(base, line)?,
                    label,
                    "video",
                    dimensions.map(|d| d.0),
                    dimensions.map(|d| d.1),
                    a.get("AUDIO").cloned(),
                    None,
                ));
            }
        }
        if pending.is_some() || choices.is_empty() || choices.len() > 256 {
            bail!("invalid_media_manifest");
        }
        return Ok(Discovery {
            kind: "hls".into(),
            choices,
        });
    }
    if !lines.contains(&"#EXT-X-ENDLIST") {
        bail!("live_media_unsupported");
    }
    let mut pieces = Vec::new();
    let mut pending_duration = None;
    let mut duration = 0.0;
    let mut pending_range = None;
    let mut previous = None;
    let mut map_seen = false;
    for line in &lines {
        if let Some(value) = line.strip_prefix("#EXTINF:") {
            if pending_duration.is_some() {
                bail!("invalid_media_manifest");
            }
            let seconds = value
                .split(',')
                .next()
                .unwrap_or("")
                .parse::<f64>()
                .context("invalid_media_manifest")?;
            if !seconds.is_finite() || seconds <= 0.0 {
                bail!("invalid_media_manifest");
            }
            pending_duration = Some(seconds);
        } else if let Some(value) = line.strip_prefix("#EXT-X-BYTERANGE:") {
            if pending_range.is_some() {
                bail!("invalid_media_range");
            }
            pending_range = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("#EXT-X-MAP:") {
            if map_seen || !pieces.is_empty() {
                bail!("unsupported_media_timeline");
            }
            map_seen = true;
            let a = attrs(value)?;
            let url = joined(base, a.get("URI").context("invalid_media_manifest")?)?;
            let range = a
                .get("BYTERANGE")
                .map(|r| byte_range(r, None, &url))
                .transpose()?;
            pieces.push(Piece { url, range });
        } else if !line.starts_with('#') {
            duration += pending_duration.take().context("invalid_media_manifest")?;
            let url = joined(base, line)?;
            let range = pending_range
                .take()
                .map(|r| byte_range(&r, previous.clone(), &url))
                .transpose()?;
            previous = range.map(|(o, l)| (url.clone(), o + l));
            pieces.push(Piece { url, range });
            if pieces.len() > MAX_PIECES {
                bail!("media_too_many_segments");
            }
        }
    }
    if pieces.is_empty() || pending_duration.is_some() || pending_range.is_some() || duration <= 0.0
    {
        bail!("invalid_media_manifest");
    }
    let track = Track {
        role: "muxed".into(),
        pieces,
        width: None,
        height: None,
        duration: Some(duration),
    };
    Ok(Discovery {
        kind: "hls".into(),
        choices: vec![choice(
            base.into(),
            "Original VOD rendition".into(),
            "muxed",
            None,
            None,
            None,
            Some(track),
        )],
    })
}

#[derive(Default, Clone)]
struct Node {
    name: String,
    attrs: BTreeMap<String, String>,
    children: Vec<Node>,
    text: String,
}
impl Node {
    fn get(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|n| n.name == name)
    }
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(String::as_str)
    }
}
fn xml(text: &str) -> Result<Node> {
    use quick_xml::{Reader, events::Event};
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);
    let mut stack = vec![Node::default()];
    let mut count = 0;
    loop {
        match reader.read_event().context("invalid_media_manifest")? {
            Event::Start(e) | Event::Empty(e) => {
                // Empty is distinguished by its lexical end, before consuming the event.
                let end = reader.buffer_position() as usize;
                let empty = text.as_bytes().get(end.saturating_sub(2)) == Some(&b'/');
                let name = e.name().as_ref().to_owned();
                if name.contains(':') || name == "ContentProtection" || name == "SegmentBase" {
                    bail!("unsupported_media_protection_or_addressing");
                }
                let mut n = Node {
                    name,
                    ..Default::default()
                };
                for a in e.attributes() {
                    let a = a.context("invalid_media_manifest")?;
                    let key = a.key.as_ref();
                    n.attrs.insert(
                        key.into(),
                        a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .context("invalid_media_manifest")?
                            .into_owned(),
                    );
                }
                count += 1;
                if count > 100_000 || stack.len() > 32 {
                    bail!("metadata_too_large");
                }
                if empty {
                    stack.last_mut().unwrap().children.push(n);
                } else {
                    stack.push(n);
                }
            }
            Event::End(_) => {
                if stack.len() < 2 {
                    bail!("invalid_media_manifest");
                }
                let n = stack.pop().unwrap();
                stack.last_mut().unwrap().children.push(n);
            }
            Event::Text(e) => {
                stack.last_mut().unwrap().text.push_str(
                    &quick_xml::escape::unescape(&e.xml10_content())
                        .context("invalid_media_manifest")?,
                );
            }
            Event::DocType(_) | Event::GeneralRef(_) => bail!("unsupported_media_xml"),
            Event::Eof => break,
            _ => {}
        }
    }
    if stack.len() != 1 || stack[0].children.len() != 1 {
        bail!("invalid_media_manifest");
    }
    Ok(stack.remove(0).children.remove(0))
}
fn duration(value: &str) -> Result<f64> {
    let value = value
        .strip_prefix("PT")
        .context("unsupported_media_duration")?;
    let mut number = String::new();
    let mut total = 0.0;
    for c in value.chars() {
        if c.is_ascii_digit() || c == '.' {
            number.push(c);
        } else {
            let n = number.parse::<f64>().context("invalid_media_manifest")?;
            total += n * match c {
                'H' => 3600.0,
                'M' => 60.0,
                'S' => 1.0,
                _ => bail!("unsupported_media_duration"),
            };
            number.clear();
        }
    }
    if !number.is_empty() || !total.is_finite() || total <= 0.0 {
        bail!("invalid_media_manifest");
    }
    Ok(total)
}
fn base(parent: &str, n: &Node) -> Result<String> {
    let all: Vec<_> = n.children.iter().filter(|c| c.name == "BaseURL").collect();
    if all.len() > 1 {
        bail!("ambiguous_media_base");
    }
    match all.first() {
        Some(b) => joined(parent, b.text.trim()),
        None => Ok(parent.into()),
    }
}
fn template(value: &str, representation: &str, number: u64, time: u64) -> Result<String> {
    let result = value
        .replace("$RepresentationID$", representation)
        .replace("$Number$", &number.to_string())
        .replace("$Time$", &time.to_string());
    if result.contains('$') {
        bail!("unsupported_media_template");
    }
    Ok(result)
}
pub fn dash(source: &str, text: &str) -> Result<Discovery> {
    let root = xml(text)?;
    if root.name != "MPD" || root.attr("type").unwrap_or("static") != "static" {
        bail!("live_media_unsupported");
    }
    let periods: Vec<_> = root
        .children
        .iter()
        .filter(|n| n.name == "Period")
        .collect();
    if periods.len() != 1 {
        bail!("unsupported_media_timeline");
    }
    let p = periods[0];
    if p.attr("start").is_some_and(|s| s != "PT0S") {
        bail!("unsupported_media_timeline");
    }
    let seconds = p
        .attr("duration")
        .or(root.attr("mediaPresentationDuration"))
        .map(duration)
        .transpose()?;
    let root_base = base(source, &root)?;
    let period_base = base(&root_base, p)?;
    let mut choices = Vec::new();
    for a in p.children.iter().filter(|n| n.name == "AdaptationSet") {
        let adaptation_base = base(&period_base, a)?;
        for r in a.children.iter().filter(|n| n.name == "Representation") {
            let url = base(&adaptation_base, r)?;
            let role = match r.attr("contentType").or(a.attr("contentType")).or_else(|| {
                r.attr("mimeType")
                    .or(a.attr("mimeType"))
                    .and_then(|m| m.split('/').next())
            }) {
                Some("video") => "video",
                Some("audio") => "audio",
                _ => continue,
            };
            let rep = r.attr("id").context("invalid_media_manifest")?;
            let width = r
                .attr("width")
                .or(a.attr("width"))
                .map(str::parse)
                .transpose()
                .context("invalid_media_manifest")?;
            let height = r
                .attr("height")
                .or(a.attr("height"))
                .map(str::parse)
                .transpose()
                .context("invalid_media_manifest")?;
            let mut pieces = Vec::new();
            if let Some(list) = r.get("SegmentList").or_else(|| a.get("SegmentList")) {
                if let Some(init) = list.get("Initialization") {
                    let init_url = joined(&url, init.attr("sourceURL").unwrap_or(""))?;
                    pieces.push(Piece {
                        range: init.attr("range").map(dash_range).transpose()?,
                        url: init_url,
                    });
                }
                for segment in list.children.iter().filter(|n| n.name == "SegmentURL") {
                    pieces.push(Piece {
                        url: joined(
                            &url,
                            segment.attr("media").context("invalid_media_manifest")?,
                        )?,
                        range: segment.attr("mediaRange").map(dash_range).transpose()?,
                    });
                }
            } else if let Some(t) = r
                .get("SegmentTemplate")
                .or_else(|| a.get("SegmentTemplate"))
            {
                let inherited = a.get("SegmentTemplate");
                let attr = |key| t.attr(key).or_else(|| inherited.and_then(|n| n.attr(key)));
                if attr("presentationTimeOffset").is_some_and(|s| s != "0") {
                    bail!("unsupported_media_timeline");
                }
                let scale = attr("timescale")
                    .unwrap_or("1")
                    .parse::<u64>()
                    .context("invalid_media_manifest")?;
                if scale == 0 {
                    bail!("invalid_media_manifest");
                }
                let first = attr("startNumber")
                    .unwrap_or("1")
                    .parse::<u64>()
                    .context("invalid_media_manifest")?;
                if let Some(init) = attr("initialization") {
                    pieces.push(Piece {
                        url: joined(&url, &template(init, rep, first, 0)?)?,
                        range: None,
                    });
                }
                let media = attr("media").context("invalid_media_manifest")?;
                let timeline = t
                    .get("SegmentTimeline")
                    .or_else(|| inherited.and_then(|n| n.get("SegmentTimeline")));
                let mut number = first;
                let mut time = 0u64;
                if let Some(timeline) = timeline {
                    for s in &timeline.children {
                        if s.name != "S" {
                            bail!("unsupported_media_timeline");
                        }
                        let d = s
                            .attr("d")
                            .context("invalid_media_manifest")?
                            .parse::<u64>()
                            .context("invalid_media_manifest")?;
                        let start = s
                            .attr("t")
                            .map(str::parse::<u64>)
                            .transpose()
                            .context("invalid_media_manifest")?
                            .unwrap_or(time);
                        if d == 0 || start != time {
                            bail!("unsupported_media_timeline");
                        }
                        let repeat = s
                            .attr("r")
                            .unwrap_or("0")
                            .parse::<u64>()
                            .context("unsupported_media_timeline")?;
                        if repeat > MAX_PIECES as u64 {
                            bail!("media_too_many_segments");
                        }
                        for _ in 0..=repeat {
                            pieces.push(Piece {
                                url: joined(&url, &template(media, rep, number, time)?)?,
                                range: None,
                            });
                            number = number.checked_add(1).context("invalid_media_manifest")?;
                            time = time.checked_add(d).context("invalid_media_manifest")?;
                            if pieces.len() > MAX_PIECES {
                                bail!("media_too_many_segments");
                            }
                        }
                    }
                } else {
                    let d = attr("duration")
                        .context("unsupported_media_addressing")?
                        .parse::<u64>()
                        .context("invalid_media_manifest")?;
                    if d == 0 {
                        bail!("invalid_media_manifest");
                    }
                    let count = (seconds.context("unsupported_media_duration")? * scale as f64
                        / d as f64)
                        .ceil();
                    if count > MAX_PIECES as f64 {
                        bail!("media_too_many_segments");
                    }
                    for _ in 0..count as u64 {
                        pieces.push(Piece {
                            url: joined(&url, &template(media, rep, number, time)?)?,
                            range: None,
                        });
                        number = number.checked_add(1).context("invalid_media_manifest")?;
                        time = time.checked_add(d).context("invalid_media_manifest")?;
                    }
                }
            } else {
                bail!("unsupported_media_addressing");
            }
            if pieces.is_empty() || pieces.len() > MAX_PIECES {
                bail!("invalid_media_manifest");
            }
            let label = format!(
                "{role} {rep} · {} · {} bit/s · {}",
                height.map_or_else(
                    || a.attr("lang").unwrap_or("original").into(),
                    |h| format!("{h}p")
                ),
                r.attr("bandwidth").unwrap_or("unknown"),
                r.attr("codecs")
                    .or(a.attr("codecs"))
                    .unwrap_or("original codecs")
            );
            let track = Track {
                role: role.into(),
                pieces,
                width,
                height,
                duration: seconds,
            };
            choices.push(choice(url, label, role, width, height, None, Some(track)));
        }
    }
    if choices.is_empty() || choices.len() > 256 {
        bail!("invalid_media_manifest");
    }
    Ok(Discovery {
        kind: "dash".into(),
        choices,
    })
}
fn dash_range(text: &str) -> Result<(u64, u64)> {
    let (a, b) = text.split_once('-').context("invalid_media_range")?;
    let a = a.parse::<u64>().context("invalid_media_range")?;
    let b = b.parse::<u64>().context("invalid_media_range")?;
    Ok((
        a,
        b.checked_sub(a)
            .and_then(|n| n.checked_add(1))
            .context("invalid_media_range")?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hls_tracks_preserve_exact_quality_language_and_byte_ranges() {
        let master=hls("https://example.org/v/master.m3u8", "#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"en\",NAME=\"English\",LANGUAGE=\"en\",URI=\"audio.m3u8\"\n#EXT-X-STREAM-INF:BANDWIDTH=2000,RESOLUTION=7680x4320,AUDIO=\"en\",CODECS=\"av01,mp4a\"\n8k.m3u8\n").unwrap();
        assert_eq!(master.choices[1].height, Some(4320));
        assert_eq!(master.choices[0].role, "audio");
        assert_eq!(master.choices[1].url, "https://example.org/v/8k.m3u8");
        let media=hls("https://example.org/v/8k.m3u8","#EXTM3U\n#EXT-X-MAP:URI=\"all.mp4\",BYTERANGE=\"20@0\"\n#EXTINF:1,\n#EXT-X-BYTERANGE:100@20\nall.mp4\n#EXTINF:2,\n#EXT-X-BYTERANGE:200\nall.mp4\n#EXT-X-ENDLIST\n").unwrap();
        let t = media.choices[0].track.as_ref().unwrap();
        assert_eq!(t.duration, Some(3.0));
        assert_eq!(t.pieces[2].range, Some((120, 200)));
    }
    #[test]
    fn hls_rejects_encryption_live_gaps_discontinuity_and_ambiguous_ranges() {
        for extra in [
            "#EXT-X-KEY:METHOD=AES-128,URI=\"key\"\n",
            "#EXT-X-DISCONTINUITY\n",
            "#EXT-X-GAP\n",
            "#EXT-X-PART:DURATION=1,URI=\"part\"\n",
            "#EXT-X-BYTERANGE:20\n",
        ] {
            assert!(
                hls(
                    "https://example.org/v.m3u8",
                    &format!("#EXTM3U\n{extra}#EXTINF:1,\na.ts\n#EXT-X-ENDLIST\n")
                )
                .is_err(),
                "{extra}"
            );
        }
        assert!(hls("https://example.org/v.m3u8", "#EXTM3U\n#EXTINF:1,\na.ts\n").is_err());
        assert!(
            hls(
                "https://example.org/v.m3u8",
                "#EXTM3U\n#EXTINF:1,\nfile:///private\n#EXT-X-ENDLIST\n"
            )
            .is_err()
        );
    }
    #[test]
    fn dash_static_segment_lists_and_bounded_templates_are_exact() {
        let d=dash("https://example.org/v/manifest.mpd",r#"<MPD type="static" mediaPresentationDuration="PT2S"><Period><AdaptationSet mimeType="video/mp4"><SegmentTemplate timescale="1" duration="1" initialization="$RepresentationID$-init.mp4" media="$RepresentationID$-$Number$.m4s"/><Representation id="8k" width="7680" height="4320" bandwidth="8000000"/></AdaptationSet></Period></MPD>"#).unwrap();
        let t = d.choices[0].track.as_ref().unwrap();
        assert_eq!(t.height, Some(4320));
        assert_eq!(t.pieces.len(), 3);
        assert_eq!(t.pieces[2].url, "https://example.org/v/8k-2.m4s");
        let d=dash("https://example.org/v.mpd",r#"<MPD><Period duration="PT2S"><AdaptationSet contentType="audio"><Representation id="en"><SegmentList><Initialization sourceURL="audio.mp4" range="0-19"/><SegmentURL media="audio.mp4" mediaRange="20-119"/></SegmentList></Representation></AdaptationSet></Period></MPD>"#).unwrap();
        assert_eq!(
            d.choices[0].track.as_ref().unwrap().pieces[1].range,
            Some((20, 100))
        );
    }
    #[test]
    fn dash_rejects_dtd_protection_multi_period_and_unbounded_timelines() {
        for text in [
            r#"<!DOCTYPE MPD [<!ENTITY x SYSTEM "file:///private">]><MPD/>"#,
            r#"<MPD type="dynamic"><Period/></MPD>"#,
            r#"<MPD><Period/><Period/></MPD>"#,
            r#"<MPD><Period><ContentProtection/></Period></MPD>"#,
            r#"<MPD><Period><AdaptationSet contentType="video"><Representation id="v"><SegmentTemplate media="$Time$.m4s"><SegmentTimeline><S d="1" r="-1"/></SegmentTimeline></SegmentTemplate></Representation></AdaptationSet></Period></MPD>"#,
        ] {
            assert!(dash("https://example.org/m.mpd", text).is_err(), "{text}");
        }
    }
}

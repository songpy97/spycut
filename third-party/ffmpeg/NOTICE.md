# FFmpeg notice for SpyCut

SpyCut executes `ffmpeg` and `ffprobe` as separate supervised processes.

Release sidecars must be built without `--enable-gpl` and without
`--enable-nonfree`. The macOS preparation script pins FFmpeg 8.0.1 source and
records its configure line. The Windows preparation script pins a BtbN
`win64-lgpl` artifact and verifies its SHA-256 digest.

FFmpeg is licensed primarily under LGPL 2.1 or later. The exact license of a
binary is determined by its configure flags and linked dependencies. Before
public redistribution, publish the corresponding source and build information
alongside the installer and have codec patent obligations reviewed for the
distribution regions.

- FFmpeg home: https://ffmpeg.org/
- FFmpeg 8.0.1 source tag mirror: https://github.com/FFmpeg/FFmpeg/tree/n8.0.1
- Pinned codeload archive SHA-256: `679aa13a19415d5ddab91e580084e3ab20c963c8240001e5cbb955a97bdd81b1`
- Windows build project: https://github.com/BtbN/FFmpeg-Builds
- Windows release tag: `autobuild-2026-07-31-14-10`
- Windows asset: `ffmpeg-N-125875-g5d4d3bdc61-win64-lgpl.zip`
- Windows archive SHA-256: `5d65df0c0ca5346d82df8ade9c2e12db45d1f978f18ff908b42f03f5223dfc90`

This notice is operational documentation, not legal advice.

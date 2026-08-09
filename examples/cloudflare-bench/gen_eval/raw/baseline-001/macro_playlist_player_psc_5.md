<!-- exp=baseline-001 task=macro_playlist_player cond=psc sample=5 model=claude-opus-4-8 cost_usd=0.042445000000000004 -->
```python
"use client"
R*

def fmt(sec):
    m = sec // 60
    s = sec % 60
    return f"{m}:{s:02d}"

PLAYLISTS = [
    {"id": 0, "name": "Chill Vibes", "tracks": [
        {"title": "Sunset Drive", "artist": "Noa Lin", "dur": 214},
        {"title": "Paper Boats", "artist": "Kavi", "dur": 187},
        {"title": "Slow Morning", "artist": "Ede", "dur": 243},
    ]},
    {"id": 1, "name": "Focus Flow", "tracks": [
        {"title": "Deep Work", "artist": "Mono Field", "dur": 305},
        {"title": "Signal", "artist": "Aria K", "dur": 198},
    ]},
    {"id": 2, "name": "Night Run", "tracks": [
        {"title": "Neon Streets", "artist": "Volt", "dur": 176},
        {"title": "Afterburn", "artist": "Circuit", "dur": 221},
        {"title": "Redline", "artist": "Volt", "dur": 259},
        {"title": "City Lights", "artist": "Mira", "dur": 233},
    ]},
]

@c
def PlaylistPlayer():
    active_id, set_active_id = us(0)
    now_playing, set_now_playing = us(None)
    is_playing, set_is_playing = us(False)

    active = [p for p in PLAYLISTS if p["id"] == active_id][0]

    def select_playlist(pid):
        set_active_id(pid)

    def play_track(track):
        set_now_playing(track)
        set_is_playing(True)

    def toggle_play():
        if now_playing is not None:
            set_is_playing(not is_playing)

    def is_current(track):
        return now_playing is not None and now_playing["title"] == track["title"] and now_playing["artist"] == track["artist"]

    return div(cn="player",
        div(cn="sidebar",
            h2(cn="sidebar-title", "Playlists"),
            ul(cn="playlist-list",
                *[li(key=p["id"],
                    cn=("playlist-item active" if p["id"] == active_id else "playlist-item"),
                    oc=lambda pid=p["id"]: select_playlist(pid),
                    span(cn="playlist-name", p["name"]),
                    span(cn="playlist-count", f"{len(p['tracks'])} tracks"),
                  ) for p in PLAYLISTS]),
        ),
        div(cn="main-panel",
            h2(cn="panel-title", active["name"]),
            ul(cn="track-list",
                *[li(key=i,
                    cn=("track-item playing" if is_current(t) else "track-item"),
                    oc=lambda tr=t: play_track(tr),
                    span(cn="track-title", t["title"]),
                    span(cn="track-artist", t["artist"]),
                    span(cn="track-dur", fmt(t["dur"])),
                  ) for i, t in enumerate(active["tracks"])]),
        ),
        div(cn="now-bar",
            div(cn="now-info",
                span(cn="now-label", "Now Playing:"),
                span(cn="now-title",
                    now_playing["title"] if now_playing is not None else "—"),
                now_playing is not None and span(cn="now-artist", now_playing["artist"]),
            ),
            button(cn="play-toggle",
                dis=(now_playing is None),
                oc=lambda: toggle_play(),
                "Pause" if is_playing else "Play"),
        ),
    )
```

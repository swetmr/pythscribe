<!-- exp=baseline-001 task=macro_playlist_player cond=psc sample=1 model=claude-opus-4-8 cost_usd=0.08884049999999999 -->
```python
"use client"
R*

playlists = [
    {"id": 0, "name": "Chill Vibes", "tracks": [
        {"title": "Sunset Drive", "artist": "Luna Bay", "duration": "3:42"},
        {"title": "Coffee & Rain", "artist": "Mellow Fox", "duration": "4:05"},
        {"title": "Slow Morning", "artist": "Haze", "duration": "2:58"},
    ]},
    {"id": 1, "name": "Workout Mix", "tracks": [
        {"title": "Push It", "artist": "Iron Pulse", "duration": "3:15"},
        {"title": "No Limits", "artist": "Volt", "duration": "3:48"},
        {"title": "Overdrive", "artist": "Redline", "duration": "4:22"},
        {"title": "Last Rep", "artist": "Titan", "duration": "3:01"},
    ]},
    {"id": 2, "name": "Focus Flow", "tracks": [
        {"title": "Deep Work", "artist": "Ambient Co", "duration": "5:10"},
        {"title": "Quiet Mind", "artist": "Nocturne", "duration": "6:03"},
        {"title": "Steady State", "artist": "Loop Theory", "duration": "4:47"},
    ]},
]

@c
def PlaylistPlayer():
    active_id, set_active_id = us(0)
    now_playing, set_now_playing = us(None)
    is_playing, set_is_playing = us(False)

    active = next((p for p in playlists if p["id"] == active_id), playlists[0])

    def select_playlist(pid):
        set_active_id(pid)

    def play_track(track):
        set_now_playing(track)
        set_is_playing(True)

    def toggle_play():
        if now_playing is not None:
            set_is_playing(not is_playing)

    return div(cn="playlist-player",
        div(cn="player-body",
            aside(cn="sidebar",
                h2(cn="sidebar-title", "Playlists"),
                ul(cn="playlist-list",
                    *[li(key=p["id"],
                        cn="playlist-item active" if p["id"] == active_id else "playlist-item",
                        oc=lambda pid=p["id"]: select_playlist(pid),
                        span(cn="playlist-name", p["name"]),
                        span(cn="playlist-count", f"{len(p['tracks'])} tracks"),
                    ) for p in playlists],
                ),
            ),
            section(cn="main-panel",
                h1(cn="main-title", active["name"]),
                ul(cn="track-list",
                    *[li(key=i,
                        cn="track-item playing" if now_playing is not None and now_playing["title"] == t["title"] else "track-item",
                        oc=lambda tr=t: play_track(tr),
                        span(cn="track-title", t["title"]),
                        span(cn="track-artist", t["artist"]),
                        span(cn="track-duration", t["duration"]),
                    ) for i, t in enumerate(active["tracks"])],
                ),
            ),
        ),
        div(cn="now-bar",
            div(cn="now-info",
                span(cn="now-label", "Now Playing:"),
                span(cn="now-title",
                    now_playing["title"] if now_playing is not None else "Nothing selected"),
            ),
            button(cn="play-toggle", dis=now_playing is None,
                oc=lambda: toggle_play(),
                "Pause" if is_playing else "Play"),
        ),
    )
```

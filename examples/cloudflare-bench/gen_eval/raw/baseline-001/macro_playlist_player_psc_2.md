<!-- exp=baseline-001 task=macro_playlist_player cond=psc sample=2 model=claude-opus-4-8 cost_usd=0.042995 -->
```python
"use client"
R*

PLAYLISTS = [
    {"id": "p1", "name": "Chill Vibes", "tracks": [
        {"title": "Sunset Drive", "artist": "Lo-Fi Collective", "duration": "3:24"},
        {"title": "Ocean Breeze", "artist": "Calm Waters", "duration": "4:02"},
        {"title": "Night Sky", "artist": "Dreamscape", "duration": "2:58"},
    ]},
    {"id": "p2", "name": "Workout Mix", "tracks": [
        {"title": "Adrenaline", "artist": "Pulse", "duration": "3:11"},
        {"title": "Full Throttle", "artist": "Redline", "duration": "3:47"},
        {"title": "No Limits", "artist": "Overdrive", "duration": "4:15"},
    ]},
    {"id": "p3", "name": "Focus Flow", "tracks": [
        {"title": "Deep Work", "artist": "Ambient Minds", "duration": "5:20"},
        {"title": "Concentration", "artist": "Study Beats", "duration": "3:33"},
        {"title": "Clarity", "artist": "Zen Garden", "duration": "4:44"},
    ]},
]


def track_key(t):
    return f"{t['title']}-{t['artist']}"


@c
def PlaylistPlayer():
    active_id, set_active_id = us(PLAYLISTS[0]["id"])
    now_playing, set_now_playing = us(None)
    is_playing, set_is_playing = us(False)

    active = next((p for p in PLAYLISTS if p["id"] == active_id), PLAYLISTS[0])

    def select_playlist(pid):
        set_active_id(pid)

    def select_track(t):
        set_now_playing(t)
        set_is_playing(True)

    def toggle_play():
        if now_playing:
            set_is_playing(not is_playing)

    return div(cn="playlist-player",
        div(cn="player-body",
            div(cn="sidebar",
                h2("Playlists"),
                ul(cn="playlist-list",
                    *[li(key=p["id"],
                        cn="playlist-item active" if p["id"] == active_id else "playlist-item",
                        oc=lambda pid=p["id"]: select_playlist(pid),
                        span(cn="playlist-name", p["name"]),
                        span(cn="playlist-count", f"{len(p['tracks'])} tracks"),
                      ) for p in PLAYLISTS]),
            ),
            div(cn="main-panel",
                h2(active["name"]),
                ul(cn="track-list",
                    *[li(key=track_key(t),
                        cn="track-item playing" if now_playing and track_key(now_playing) == track_key(t) else "track-item",
                        oc=lambda tr=t: select_track(tr),
                        span(cn="track-title", t["title"]),
                        span(cn="track-artist", t["artist"]),
                        span(cn="track-duration", t["duration"]),
                      ) for t in active["tracks"]]),
            ),
        ),
        div(cn="now-playing-bar",
            div(cn="now-playing-info",
                now_playing and span(cn="np-title", now_playing["title"]) or span(cn="np-title np-empty", "Nothing playing"),
                now_playing and span(cn="np-artist", now_playing["artist"]),
            ),
            button(cn="play-toggle", oc=lambda: toggle_play(), dis=not now_playing,
                "Pause" if is_playing else "Play"),
        ),
    )
```

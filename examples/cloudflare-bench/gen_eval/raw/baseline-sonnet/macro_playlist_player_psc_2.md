<!-- exp=baseline-sonnet task=macro_playlist_player cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.0486297 -->
```python
"use client"
R*

PLAYLISTS = [
    {"name": "Chill Vibes", "tracks": [
        {"title": "Sunset Drive", "artist": "Nova", "duration": "3:24"},
        {"title": "Ocean Breeze", "artist": "Kalm", "duration": "4:01"},
        {"title": "Late Night", "artist": "Echo", "duration": "2:58"},
    ]},
    {"name": "Workout Mix", "tracks": [
        {"title": "Pump It Up", "artist": "Rex", "duration": "3:10"},
        {"title": "Run Faster", "artist": "Vex", "duration": "3:45"},
    ]},
    {"name": "Focus Flow", "tracks": [
        {"title": "Deep Work", "artist": "Lira", "duration": "5:12"},
        {"title": "Quiet Mind", "artist": "Sen", "duration": "4:30"},
        {"title": "Clarity", "artist": "Ohm", "duration": "3:50"},
    ]},
]

@c
def PlaylistPlayer():
    active_idx, set_active_idx = us(0)
    now_playing, set_now_playing = us(None)
    is_playing, set_is_playing = us(False)

    def select_playlist(i):
        set_active_idx(i)

    def select_track(t):
        set_now_playing(t)
        set_is_playing(True)

    def toggle_play():
        set_is_playing(not is_playing)

    active_playlist = PLAYLISTS[active_idx]

    return div(cn="playlist-player",
        div(cn="sidebar",
            h2("Playlists"),
            ul(*[
                li(key=i,
                   cn=f"playlist-item{' active' if i == active_idx else ''}",
                   oc=lambda i=i: select_playlist(i),
                   span(cn="playlist-name", p["name"]),
                   span(cn="playlist-count", f"{len(p['tracks'])} tracks"),
                ) for i, p in enumerate(PLAYLISTS)
            ]),
        ),
        div(cn="main-panel",
            h2(active_playlist["name"]),
            ul(*[
                li(key=t["title"],
                   cn=f"track-item{' now-playing' if now_playing and t['title'] == now_playing['title'] else ''}",
                   oc=lambda t=t: select_track(t),
                   span(cn="track-title", t["title"]),
                   span(cn="track-artist", t["artist"]),
                   span(cn="track-duration", t["duration"]),
                ) for t in active_playlist["tracks"]
            ]),
        ),
        div(cn="bottom-bar",
            span(cn="now-playing-title", now_playing["title"] if now_playing else "No track selected"),
            button(oc=lambda: toggle_play(), dis=now_playing is None,
                   "Pause" if is_playing else "Play"),
        ),
    )
```

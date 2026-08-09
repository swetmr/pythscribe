<!-- exp=baseline-sonnet task=macro_playlist_player cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.0278097 -->
```python
"use client"
R*

PLAYLISTS = [
    {"id": 1, "name": "Chill Vibes", "tracks": [
        {"id": 1, "title": "Sunset Drive", "artist": "Nora Lane", "duration": "3:24"},
        {"id": 2, "title": "Soft Rain", "artist": "Milo Reyes", "duration": "2:58"},
        {"id": 3, "title": "Late Night Talk", "artist": "The Quiet Room", "duration": "4:02"},
    ]},
    {"id": 2, "name": "Workout Mix", "tracks": [
        {"id": 4, "title": "Pump It Up", "artist": "Kara Voss", "duration": "3:10"},
        {"id": 5, "title": "Run Faster", "artist": "Delta Crew", "duration": "3:45"},
    ]},
    {"id": 3, "name": "Focus Flow", "tracks": [
        {"id": 6, "title": "Deep Work", "artist": "Ambient Set", "duration": "5:12"},
        {"id": 7, "title": "Quiet Mind", "artist": "Ambient Set", "duration": "4:30"},
        {"id": 8, "title": "Clarity", "artist": "Nils Park", "duration": "3:55"},
    ]},
]

@c
def PlaylistPlayer():
    active_id, set_active_id = us(PLAYLISTS[0]["id"])
    now_playing, set_now_playing = us(None)
    is_playing, set_is_playing = us(False)

    active_playlist = [p for p in PLAYLISTS if p["id"] == active_id][0]

    def select_track(track):
        set_now_playing(track)
        set_is_playing(True)

    def toggle_play():
        set_is_playing(not is_playing)

    return div(cn="player-app",
        div(cn="sidebar",
            h2("Playlists"),
            ul(*[
                li(key=p["id"], cn=f"playlist-item{' active' if p['id'] == active_id else ''}",
                   oc=lambda p=p: set_active_id(p["id"]),
                   div(cn="playlist-name", p["name"]),
                   div(cn="playlist-count", f"{len(p['tracks'])} tracks"),
                ) for p in PLAYLISTS
            ]),
        ),
        div(cn="main-panel",
            h2(active_playlist["name"]),
            ul(cn="track-list", *[
                li(key=t["id"],
                   cn=f"track-item{' now-playing' if now_playing and t['id'] == now_playing['id'] else ''}",
                   oc=lambda t=t: select_track(t),
                   span(cn="track-title", t["title"]),
                   span(cn="track-artist", t["artist"]),
                   span(cn="track-duration", t["duration"]),
                ) for t in active_playlist["tracks"]
            ]),
        ),
        div(cn="bottom-bar",
            span(cn="now-playing-title", now_playing["title"] if now_playing else "Nothing playing"),
            button(cn="play-toggle", oc=lambda: toggle_play(), dis=now_playing is None,
                   "Pause" if is_playing else "Play"),
        ),
    )
```

<!-- exp=macrov2-sonnet task=macro_playlist_player cond=psc sample=1 model=claude-sonnet-5 cost_usd=0.0610956 -->
```python
"use client"
R*

PLAYLISTS = [
    {"id": "p1", "name": "Chill Vibes", "tracks": [
        {"id": "t1", "title": "Sunset Drive", "artist": "Nova Sound", "duration": "3:24"},
        {"id": "t2", "title": "Ocean Air", "artist": "Lull", "duration": "2:58"},
        {"id": "t3", "title": "Slow Burn", "artist": "Ember", "duration": "4:01"},
    ]},
    {"id": "p2", "name": "Workout Mix", "tracks": [
        {"id": "t4", "title": "Pump It Up", "artist": "Voltage", "duration": "3:10"},
        {"id": "t5", "title": "Fast Lane", "artist": "Rev", "duration": "3:45"},
        {"id": "t6", "title": "Iron Grip", "artist": "Titan", "duration": "2:50"},
        {"id": "t7", "title": "Sprint", "artist": "Voltage", "duration": "3:02"},
    ]},
    {"id": "p3", "name": "Focus Flow", "tracks": [
        {"id": "t8", "title": "Deep Work", "artist": "Quiet Mind", "duration": "5:12"},
        {"id": "t9", "title": "Still Point", "artist": "Calibra", "duration": "4:33"},
    ]},
    {"id": "p4", "name": "Road Trip", "tracks": [
        {"id": "t10", "title": "Open Highway", "artist": "Dust Trail", "duration": "3:38"},
        {"id": "t11", "title": "Wanderlust", "artist": "Nova Sound", "duration": "3:15"},
        {"id": "t12", "title": "Backroads", "artist": "Ember", "duration": "3:50"},
    ]},
]

@c
def PlaylistPlayer():
    active_id, set_active_id = us(PLAYLISTS[0]["id"])
    now_playing, set_now_playing = us(None)
    is_playing, set_is_playing = us(False)

    active_playlist = next(p for p in PLAYLISTS if p["id"] == active_id)

    def select_playlist(pid):
        set_active_id(pid)

    def select_track(track):
        set_now_playing(track)
        set_is_playing(True)

    def toggle_play():
        set_is_playing(not is_playing)

    return div(cn="player-app",
        div(cn="sidebar",
            h2("Playlists"),
            ul(cn="playlist-list",
                *[li(key=p["id"],
                     cn=f"playlist-item{' active' if p['id'] == active_id else ''}",
                     oc=lambda p=p: select_playlist(p["id"]),
                     div(cn="playlist-name", p["name"]),
                     div(cn="playlist-count", f"{len(p['tracks'])} tracks"),
                   ) for p in PLAYLISTS],
            ),
        ),
        div(cn="main-panel",
            h2(active_playlist["name"]),
            ul(cn="track-list",
                *[li(key=t["id"],
                     cn=f"track-item{' now-playing' if now_playing and t['id'] == now_playing['id'] else ''}",
                     oc=lambda t=t: select_track(t),
                     span(cn="track-title", t["title"]),
                     span(cn="track-artist", t["artist"]),
                     span(cn="track-duration", t["duration"]),
                   ) for t in active_playlist["tracks"]],
            ),
        ),
        div(cn="bottom-bar",
            div(cn="now-playing-info",
                now_playing["title"] if now_playing else "No track selected",
            ),
            button(cn="play-toggle", oc=lambda: toggle_play(), dis=now_playing is None,
                "Pause" if is_playing else "Play",
            ),
        ),
    )
```

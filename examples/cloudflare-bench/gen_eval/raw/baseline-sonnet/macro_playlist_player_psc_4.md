<!-- exp=baseline-sonnet task=macro_playlist_player cond=psc sample=4 model=claude-sonnet-5 cost_usd=0.037214699999999996 -->
```python
"use client"
R*

PLAYLISTS = [
    {"id": 1, "name": "Chill Vibes", "tracks": [
        {"id": 101, "title": "Sunset Drive", "artist": "Nova Kite", "duration": "3:24"},
        {"id": 102, "title": "Ocean Breeze", "artist": "Lena Marsh", "duration": "4:01"},
        {"id": 103, "title": "Late Night Coffee", "artist": "Tomasz Roe", "duration": "2:58"},
    ]},
    {"id": 2, "name": "Workout Mix", "tracks": [
        {"id": 201, "title": "Iron Pulse", "artist": "Dax Ferro", "duration": "3:12"},
        {"id": 202, "title": "Sprint", "artist": "Rikka Vane", "duration": "2:45"},
        {"id": 203, "title": "Overdrive", "artist": "Marlo Kip", "duration": "3:37"},
        {"id": 204, "title": "Push Harder", "artist": "Dax Ferro", "duration": "3:05"},
    ]},
    {"id": 3, "name": "Focus Flow", "tracks": [
        {"id": 301, "title": "Quiet Circuits", "artist": "Ansel Bray", "duration": "5:10"},
        {"id": 302, "title": "Deep Work", "artist": "Ines Colt", "duration": "4:44"},
    ]},
    {"id": 4, "name": "Road Trip", "tracks": [
        {"id": 401, "title": "Highway Lights", "artist": "Vess & Coe", "duration": "3:50"},
        {"id": 402, "title": "Dust and Gold", "artist": "Marlo Kip", "duration": "3:29"},
        {"id": 403, "title": "Windows Down", "artist": "Nova Kite", "duration": "2:52"},
    ]},
]

def find_playlist(playlist_id):
    for p in PLAYLISTS:
        if p["id"] == playlist_id:
            return p
    return PLAYLISTS[0]

@c
def PlaylistPlayer():
    active_id, set_active_id = us(PLAYLISTS[0]["id"])
    now_playing, set_now_playing = us(None)
    is_playing, set_is_playing = us(False)

    active_playlist = find_playlist(active_id)

    def select_playlist(pid):
        set_active_id(pid)

    def select_track(track):
        set_now_playing(track)
        set_is_playing(True)

    def toggle_play():
        if now_playing:
            set_is_playing(not is_playing)

    return div(cn="playlist-player",
        div(cn="sidebar",
            h2("Playlists"),
            ul(cn="playlist-list",
                *[li(key=p["id"],
                     cn=f"playlist-item{' active' if p['id'] == active_id else ''}",
                     oc=lambda pid=p["id"]: select_playlist(pid),
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
            div(cn="now-playing-title",
                now_playing["title"] + " — " + now_playing["artist"] if now_playing else "No track selected",
            ),
            button(cn="play-toggle", oc=lambda: toggle_play(), dis=now_playing is None,
                "Pause" if is_playing else "Play",
            ),
        ),
    )
```

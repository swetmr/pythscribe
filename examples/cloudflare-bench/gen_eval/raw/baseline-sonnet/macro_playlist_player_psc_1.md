<!-- exp=baseline-sonnet task=macro_playlist_player cond=psc sample=1 model=claude-sonnet-5 cost_usd=0.062127 -->
```python
"use client"
R*

PLAYLISTS = [
    {"id": 1, "name": "Chill Vibes", "tracks": [
        {"id": 101, "title": "Sunset Drive", "artist": "Kai Lumen", "duration": "3:24"},
        {"id": 102, "title": "Slow Tide", "artist": "Marina Voss", "duration": "4:02"},
        {"id": 103, "title": "Quiet Hours", "artist": "Nomi Rae", "duration": "2:58"},
    ]},
    {"id": 2, "name": "Workout Mix", "tracks": [
        {"id": 201, "title": "Overdrive", "artist": "Torque", "duration": "3:10"},
        {"id": 202, "title": "Pulse Check", "artist": "Rell Vance", "duration": "3:45"},
        {"id": 203, "title": "Sprint", "artist": "Dax Renner", "duration": "2:40"},
        {"id": 204, "title": "Ironclad", "artist": "Torque", "duration": "4:15"},
    ]},
    {"id": 3, "name": "Late Night Jazz", "tracks": [
        {"id": 301, "title": "Blue Room", "artist": "Elis Marchand", "duration": "5:12"},
        {"id": 302, "title": "Smoke Signal", "artist": "Theo Vane", "duration": "4:33"},
    ]},
    {"id": 4, "name": "Road Trip", "tracks": [
        {"id": 401, "title": "Highway Song", "artist": "Rue Callahan", "duration": "3:50"},
        {"id": 402, "title": "Windows Down", "artist": "Jonah Pike", "duration": "3:18"},
        {"id": 403, "title": "Miles Ahead", "artist": "Rue Callahan", "duration": "4:05"},
    ]},
]

def find_playlist(playlist_id):
    for p in PLAYLISTS:
        if p["id"] == playlist_id:
            return p
    return PLAYLISTS[0]

def find_track(tracks, track_id):
    for t in tracks:
        if t["id"] == track_id:
            return t
    return None

@c
def PlaylistPlayer():
    active_id, set_active_id = us(PLAYLISTS[0]["id"])
    now_playing_id, set_now_playing_id = us(None)
    is_playing, set_is_playing = us(False)

    active_playlist = find_playlist(active_id)
    now_playing_track = find_track(active_playlist["tracks"], now_playing_id)

    def select_playlist(pid):
        set_active_id(pid)

    def select_track(tid):
        set_now_playing_id(tid)
        set_is_playing(True)

    def toggle_play():
        if now_playing_track:
            set_is_playing(not is_playing)

    return div(cn="playlist-player",
        div(cn="sidebar",
            h2(cn="sidebar-title", "Playlists"),
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
            h2(cn="main-title", active_playlist["name"]),
            ul(cn="track-list",
                *[li(key=t["id"],
                     cn=f"track-item{' now-playing' if t['id'] == now_playing_id else ''}",
                     oc=lambda tid=t["id"]: select_track(tid),
                     span(cn="track-title", t["title"]),
                     span(cn="track-artist", t["artist"]),
                     span(cn="track-duration", t["duration"]),
                   ) for t in active_playlist["tracks"]],
            ),
        ),
        div(cn="bottom-bar",
            div(cn="now-playing-info",
                now_playing_track["title"] if now_playing_track else "No track selected",
            ),
            button(cn="play-toggle", dis=now_playing_track is None,
                   oc=lambda: toggle_play(),
                   "Pause" if is_playing else "Play"),
        ),
    )
```

<!-- exp=baseline-sonnet task=macro_playlist_player cond=psc sample=5 model=claude-sonnet-5 cost_usd=0.08015970000000001 -->
```python
"use client"
R*

PLAYLISTS = [
    {"id": 1, "name": "Chill Vibes", "tracks": [
        {"id": 101, "title": "Sunset Drive", "artist": "Nora Lane", "duration": 214},
        {"id": 102, "title": "Ocean Air", "artist": "Kai Reeves", "duration": 187},
        {"id": 103, "title": "Slow Burn", "artist": "Mira Voss", "duration": 245},
    ]},
    {"id": 2, "name": "Workout Mix", "tracks": [
        {"id": 201, "title": "Overdrive", "artist": "Rex Talon", "duration": 198},
        {"id": 202, "title": "Iron Pulse", "artist": "Dane Cross", "duration": 176},
        {"id": 203, "title": "Sprint", "artist": "Vex Nova", "duration": 163},
        {"id": 204, "title": "Push Limit", "artist": "Rex Talon", "duration": 210},
    ]},
    {"id": 3, "name": "Focus Flow", "tracks": [
        {"id": 301, "title": "Quiet Mind", "artist": "Ada Sol", "duration": 260},
        {"id": 302, "title": "Deep Work", "artist": "Ilan Bree", "duration": 302},
        {"id": 303, "title": "Clear Signal", "artist": "Ada Sol", "duration": 221},
    ]},
]

def find_playlist(pid):
    for p in PLAYLISTS:
        if p["id"] == pid:
            return p

def format_duration(seconds):
    return f"{seconds // 60}:{seconds % 60:02d}"

@c
def PlaylistPlayer():
    active_id, set_active_id = us(PLAYLISTS[0]["id"])
    now_playing, set_now_playing = us(None)
    is_playing, set_is_playing = us(False)

    active_playlist = find_playlist(active_id)

    def select_track(track):
        set_now_playing(track)
        set_is_playing(True)

    def toggle_play():
        set_is_playing(not is_playing)

    return div(cn="playlist-player",
        div(cn="sidebar",
            h2("Playlists"),
            ul(*[
                li(key=p["id"],
                   cn=f"playlist-item {'active' if p['id'] == active_id else ''}",
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
                   cn=f"track-item {'now-playing' if now_playing and now_playing['id'] == t['id'] else ''}",
                   oc=lambda t=t: select_track(t),
                   div(cn="track-title", t["title"]),
                   div(cn="track-artist", t["artist"]),
                   div(cn="track-duration", format_duration(t["duration"])),
                ) for t in active_playlist["tracks"]
            ]),
        ),
        div(cn="bottom-bar",
            span(cn="now-playing-title",
                 now_playing["title"] if now_playing else "No track selected"),
            button(oc=lambda: toggle_play(), dis=now_playing is None,
                   "Pause" if is_playing else "Play"),
        ),
    )
```

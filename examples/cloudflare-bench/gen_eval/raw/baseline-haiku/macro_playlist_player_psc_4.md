<!-- exp=baseline-haiku task=macro_playlist_player cond=psc sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0317444 -->
```python
"use client"
R*

PLAYLISTS = [
    {"id": 1, "name": "Chill Vibes", "tracks": [
        {"id": 11, "title": "Sunset Dreams", "artist": "Luna Echo", "duration": "3:42"},
        {"id": 12, "title": "Midnight Flow", "artist": "Neon Nights", "duration": "4:15"},
        {"id": 13, "title": "Silent Blue", "artist": "Azure Sound", "duration": "3:28"},
    ]},
    {"id": 2, "name": "Focus Beats", "tracks": [
        {"id": 21, "title": "Code Session", "artist": "Synth Wave", "duration": "5:00"},
        {"id": 22, "title": "Deep Focus", "artist": "Brain Waves", "duration": "4:45"},
    ]},
    {"id": 3, "name": "Party Mix", "tracks": [
        {"id": 31, "title": "Dance Floor", "artist": "Electric Pulse", "duration": "3:35"},
        {"id": 32, "title": "Rhythm Rush", "artist": "Beat Masters", "duration": "3:50"},
        {"id": 33, "title": "Night Energy", "artist": "Groove Kings", "duration": "4:20"},
        {"id": 34, "title": "Peak Hours", "artist": "Sound Surge", "duration": "3:15"},
    ]},
]

@c
def PlaylistPlayer():
    active_id, set_active_id = us(1)
    now_playing_id, set_now_playing_id = us(None)
    is_playing, set_is_playing = us(False)
    
    active_pl = [p for p in PLAYLISTS if p["id"] == active_id][0]
    
    now_playing = None
    if now_playing_id:
        matches = [t for t in active_pl["tracks"] if t["id"] == now_playing_id]
        now_playing = matches[0] if matches else None
    
    return div(cn="player-container",
        div(cn="sidebar",
            h3("Playlists"),
            div(cn="playlist-list",
                *[div(
                    cn=f"playlist-item {'active' if p['id'] == active_id else ''}",
                    oc=lambda pid=p["id"]: set_active_id(pid),
                    div(cn="playlist-name", p["name"]),
                    div(cn="playlist-count", f"{len(p['tracks'])} tracks"),
                ) for p in PLAYLISTS]
            ),
        ),
        div(cn="main-panel",
            h2(active_pl["name"]),
            div(cn="tracks-list",
                *[div(
                    cn=f"track-item {'now-playing' if t['id'] == now_playing_id else ''}",
                    oc=lambda tid=t["id"]: set_now_playing_id(tid),
                    div(cn="track-title", t["title"]),
                    div(cn="track-artist", t["artist"]),
                    div(cn="track-duration", t["duration"]),
                ) for t in active_pl["tracks"]]
            ),
        ),
        div(cn="player-bar",
            div(cn="now-playing-info",
                div(cn="now-playing-title", now_playing["title"] if now_playing else "No track selected"),
                div(cn="now-playing-artist", now_playing["artist"] if now_playing else ""),
            ),
            button(
                cn=f"play-pause-btn {'playing' if is_playing else 'paused'}",
                oc=lambda: set_is_playing(not is_playing),
                "▶" if not is_playing else "⏸"
            ),
        ),
    )
```

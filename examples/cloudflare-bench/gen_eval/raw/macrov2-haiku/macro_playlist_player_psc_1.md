<!-- exp=macrov2-haiku task=macro_playlist_player cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0697336 -->
```python
"use client"
R*

PLAYLISTS = [
    {"id": 1, "name": "Chill Vibes", "tracks": [
        {"id": 1, "title": "Blue Hour", "artist": "Artist A", "duration": "3:45"},
        {"id": 2, "title": "Golden Light", "artist": "Artist B", "duration": "4:12"},
        {"id": 3, "title": "Sunset Dreams", "artist": "Artist C", "duration": "3:58"},
    ]},
    {"id": 2, "name": "Workout Mix", "tracks": [
        {"id": 4, "title": "Pump It Up", "artist": "Artist D", "duration": "3:30"},
        {"id": 5, "title": "Energy Flow", "artist": "Artist E", "duration": "4:05"},
        {"id": 6, "title": "Peak Hours", "artist": "Artist F", "duration": "3:52"},
    ]},
    {"id": 3, "name": "Study Session", "tracks": [
        {"id": 7, "title": "Focus Mode", "artist": "Artist G", "duration": "5:00"},
        {"id": 8, "title": "Deep Thoughts", "artist": "Artist H", "duration": "4:30"},
        {"id": 9, "title": "Mind Palace", "artist": "Artist I", "duration": "4:15"},
    ]},
]

@c
def PlaylistPlayer():
    active_id, set_active_id = us(PLAYLISTS[0]["id"])
    playing_id, set_playing_id = us(None)
    is_playing, set_is_playing = us(False)
    
    active_pl = next((p for p in PLAYLISTS if p["id"] == active_id), None)
    playing_track = next((t for p in PLAYLISTS for t in p["tracks"] if t["id"] == playing_id), None)
    
    return div(cn="player-container",
        div(cn="sidebar",
            h3("Playlists"),
            div(cn="playlist-list",
                *[div(cn="playlist-item " + ("active" if p["id"] == active_id else ""),
                      oc=lambda: set_active_id(p["id"]),
                      div(cn="playlist-name", p["name"]),
                      div(cn="track-count", f"{len(p['tracks'])} tracks"),
                  ) for p in PLAYLISTS],
            ),
        ),
        div(cn="main-panel",
            h2(active_pl["name"]) if active_pl else h2("No Playlist"),
            div(cn="track-list",
                *[div(cn="track-item " + ("now-playing" if t["id"] == playing_id else ""),
                      oc=lambda: set_playing_id(t["id"]),
                      div(cn="track-title", t["title"]),
                      div(cn="track-artist", t["artist"]),
                      div(cn="track-duration", t["duration"]),
                  ) for t in (active_pl["tracks"] if active_pl else [])],
            ),
        ),
        div(cn="bottom-bar",
            div(cn="now-playing",
                div(cn="np-title", f"Now Playing: {playing_track['title'] if playing_track else 'No track selected'}"),
            ),
            button(oc=lambda: set_is_playing(not is_playing), cn="play-button",
                   "⏸ Pause" if is_playing else "▶ Play"),
        ),
    )
```

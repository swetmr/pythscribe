<!-- exp=macrov2-haiku task=macro_playlist_player cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0568619 -->
```python
"use client"
R*

playlists = [
    {"id": 1, "name": "Chill Vibes", "tracks": [0, 1, 2]},
    {"id": 2, "name": "Workout Mix", "tracks": [3, 4, 5]},
    {"id": 3, "name": "Sleep Sounds", "tracks": [6, 7, 8]},
]

tracks = [
    {"id": 0, "title": "Midnight Dreams", "artist": "Luna Echo", "duration": "3:24"},
    {"id": 1, "title": "Peaceful Waves", "artist": "Ocean Sound", "duration": "4:12"},
    {"id": 2, "title": "Starlight Path", "artist": "Cosmic Dust", "duration": "3:45"},
    {"id": 3, "title": "Energy Burst", "artist": "Thunder Heart", "duration": "3:30"},
    {"id": 4, "title": "Pump It Up", "artist": "Beat Masters", "duration": "4:02"},
    {"id": 5, "title": "Run Fast", "artist": "Velocity", "duration": "3:15"},
    {"id": 6, "title": "Drift Away", "artist": "Serenity", "duration": "5:10"},
    {"id": 7, "title": "Soft Whisper", "artist": "Hush", "duration": "4:45"},
    {"id": 8, "title": "Dreamland", "artist": "Cloud Nine", "duration": "3:55"},
]

@c
def PlaylistPlayer():
    active_playlist, set_active_playlist = us(playlists[0]["id"])
    now_playing, set_now_playing = us(None)
    is_playing, set_is_playing = us(False)

    playlist = [p for p in playlists if p["id"] == active_playlist][0]
    playlist_tracks = [tracks[i] for i in playlist["tracks"]]
    
    def toggle_play():
        set_is_playing(not is_playing)
    
    def play_track(track_id):
        set_now_playing(track_id)
        set_is_playing(True)

    now_playing_track = None
    if now_playing is not None:
        found = [t for t in tracks if t["id"] == now_playing]
        if found:
            now_playing_track = found[0]

    return div(cn="player-container",
        aside(cn="sidebar",
            h2("Playlists"),
            div(cn="playlist-list",
                *[div(key=p["id"], cn=f"playlist-item {'active' if p['id'] == active_playlist else ''}",
                       oc=lambda: set_active_playlist(p["id"]),
                       div(cn="playlist-name", p["name"]),
                       div(cn="playlist-count", f"{len(p['tracks'])} tracks"),
                   )
                   for p in playlists]
            ),
        ),
        section(cn="main-content",
            div(cn="header", h2(playlist["name"])),
            div(cn="track-list",
                *[div(key=t["id"], cn=f"track-item {'now-playing' if t['id'] == now_playing else ''}",
                       oc=lambda: play_track(t["id"]),
                       div(cn="track-info",
                           div(cn="track-title", t["title"]),
                           div(cn="track-artist", t["artist"]),
                       ),
                       div(cn="track-duration", t["duration"]),
                   )
                   for t in playlist_tracks]
            ),
        ),
        footer(cn="player-bar",
            div(cn="now-playing-info",
                div(cn="now-playing-title", 
                    now_playing_track["title"] if now_playing_track else "No track selected"),
                div(cn="now-playing-artist",
                    now_playing_track["artist"] if now_playing_track else ""),
            ),
            button(cn=f"play-button {'playing' if is_playing else 'paused'}",
                   oc=lambda: toggle_play(),
                   "▶ Play" if not is_playing else "⏸ Pause"),
        ),
    )
```

<!-- exp=baseline-haiku task=macro_playlist_player cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0479323 -->
```python
"use client"
R*

@c
def PlaylistPlayer():
    playlists = [
        {"id": 1, "name": "Chill Vibes", "tracks": [
            {"id": 101, "title": "Midnight Dreams", "artist": "Luna Hayes", "duration": "3:45"},
            {"id": 102, "title": "Ocean Breeze", "artist": "Coastal Crew", "duration": "4:12"},
            {"id": 103, "title": "Starlight", "artist": "Night Sky", "duration": "3:28"},
        ]},
        {"id": 2, "name": "Workout Mix", "tracks": [
            {"id": 201, "title": "Power Up", "artist": "Energy Burst", "duration": "3:32"},
            {"id": 202, "title": "Run Free", "artist": "Sprint Masters", "duration": "3:55"},
            {"id": 203, "title": "Intensity", "artist": "Pump It Up", "duration": "4:02"},
        ]},
        {"id": 3, "name": "Jazz Evening", "tracks": [
            {"id": 301, "title": "Smooth Talk", "artist": "Blue Notes", "duration": "4:30"},
            {"id": 302, "title": "Improvisation", "artist": "Jazz Quartet", "duration": "5:15"},
            {"id": 303, "title": "Melancholy", "artist": "Soft Keys", "duration": "4:48"},
        ]},
    ]
    
    active_playlist_id, set_active_playlist_id = us(1)
    now_playing_id, set_now_playing_id = us(101)
    is_playing, set_is_playing = us(False)
    
    def select_playlist(playlist_id):
        set_active_playlist_id(playlist_id)
        playlist = next(p for p in playlists if p["id"] == playlist_id)
        set_now_playing_id(playlist["tracks"][0]["id"])
        set_is_playing(True)
    
    active_playlist = next(p for p in playlists if p["id"] == active_playlist_id)
    now_playing_track = next((t for t in active_playlist["tracks"] if t["id"] == now_playing_id), None)
    
    def select_track(track_id):
        set_now_playing_id(track_id)
        set_is_playing(True)
    
    return div(cn="player-container",
        div(cn="sidebar",
            h3("Playlists"),
            ul(cn="playlist-list", *[
                li(key=f"pl-{p['id']}", cn="playlist-item" + (" active" if p["id"] == active_playlist_id else ""),
                   oc=lambda pid=p["id"]: select_playlist(pid),
                   div(cn="playlist-name", p["name"]),
                   div(cn="playlist-count", f"{len(p['tracks'])} tracks"),
                )
                for p in playlists
            ]),
        ),
        div(cn="main-panel",
            h2(active_playlist["name"]),
            ul(cn="track-list", *[
                li(key=f"tr-{t['id']}", cn="track-item" + (" now-playing" if t["id"] == now_playing_id else ""),
                   oc=lambda tid=t["id"]: select_track(tid),
                   div(cn="track-title", t["title"]),
                   div(cn="track-artist", t["artist"]),
                   div(cn="track-duration", t["duration"]),
                )
                for t in active_playlist["tracks"]
            ]),
        ),
        div(cn="bottom-bar",
            div(cn="now-playing-info",
                p(cn="now-playing-title", now_playing_track["title"] if now_playing_track else "No track"),
                p(cn="now-playing-artist", now_playing_track["artist"] if now_playing_track else ""),
            ),
            button(cn="play-pause-btn", oc=lambda: set_is_playing(not is_playing),
                "▶ Play" if not is_playing else "⏸ Pause"
            ),
        ),
    )
```

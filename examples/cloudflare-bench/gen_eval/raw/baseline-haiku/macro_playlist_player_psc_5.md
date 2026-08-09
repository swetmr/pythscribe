<!-- exp=baseline-haiku task=macro_playlist_player cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0411144 -->
```python
"use client"
R*

@c
def PlaylistPlayer():
    playlists = [
        {
            "id": 1,
            "name": "Chill Vibes",
            "tracks": [
                {"id": "t1", "title": "Midnight Dreams", "artist": "Luna Echo", "duration": "3:45"},
                {"id": "t2", "title": "Silent Stars", "artist": "Cosmic Wave", "duration": "4:12"},
                {"id": "t3", "title": "Sunset Glow", "artist": "Amber Light", "duration": "3:28"},
            ]
        },
        {
            "id": 2,
            "name": "Workout Mix",
            "tracks": [
                {"id": "t4", "title": "Pump It Up", "artist": "Electric Pulse", "duration": "3:15"},
                {"id": "t5", "title": "Feel the Beat", "artist": "Thunder Road", "duration": "3:50"},
                {"id": "t6", "title": "High Energy", "artist": "Power Drive", "duration": "2:58"},
            ]
        },
        {
            "id": 3,
            "name": "Road Trip",
            "tracks": [
                {"id": "t7", "title": "Highway Dreams", "artist": "Midnight Traveler", "duration": "4:05"},
                {"id": "t8", "title": "Endless Roads", "artist": "Journey Home", "duration": "3:42"},
                {"id": "t9", "title": "Summer Nights", "artist": "Horizon Sound", "duration": "3:33"},
            ]
        },
    ]
    
    active_playlist_id, set_active_playlist_id = us(1)
    now_playing_id, set_now_playing_id = us(None)
    is_playing, set_is_playing = us(False)
    
    active_playlist = next((p for p in playlists if p["id"] == active_playlist_id), None)
    now_playing = None
    if active_playlist:
        now_playing = next((t for t in active_playlist["tracks"] if t["id"] == now_playing_id), None)
    
    return div(cn="player-container",
        div(cn="sidebar",
            div(cn="sidebar-header", "Playlists"),
            div(cn="playlist-list",
                *[div(cn=f"playlist-item{' active' if p['id'] == active_playlist_id else ''}", 
                      oc=lambda pid=p["id"]: set_active_playlist_id(pid),
                      div(cn="playlist-name", p["name"]),
                      div(cn="track-count", f"{len(p['tracks'])} tracks"),
                  ) for p in playlists]
            ),
        ),
        div(cn="main-panel",
            div(cn="panel-header", active_playlist["name"] if active_playlist else "No Playlist"),
            div(cn="tracks-list",
                *[div(cn=f"track-item{' now-playing' if t['id'] == now_playing_id else ''}", 
                      oc=lambda tid=t["id"]: set_now_playing_id(tid),
                      div(cn="track-info",
                          div(cn="track-title", t["title"]),
                          div(cn="track-artist", t["artist"]),
                      ),
                      div(cn="track-duration", t["duration"]),
                  ) for t in (active_playlist["tracks"] if active_playlist else [])]
            ),
        ),
        div(cn="bottom-bar",
            div(cn="now-playing-info",
                div(cn="np-title", now_playing["title"] if now_playing else "No track selected"),
            ),
            button(oc=lambda: set_is_playing(not is_playing), cn="play-button",
                   "▶ Play" if not is_playing else "⏸ Pause"),
        ),
    )
```

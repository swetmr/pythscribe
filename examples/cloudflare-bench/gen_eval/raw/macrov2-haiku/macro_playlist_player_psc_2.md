<!-- exp=macrov2-haiku task=macro_playlist_player cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0670219 -->
```python
"use client"
R*

@c
def PlaylistPlayer():
    playlists = [
        {
            "name": "Chill Vibes",
            "tracks": [
                {"id": 1, "title": "Sunset Dreams", "artist": "Luna Echo", "duration": "3:45"},
                {"id": 2, "title": "Ocean Waves", "artist": "The Drift", "duration": "4:12"},
                {"id": 3, "title": "Starlight", "artist": "Nova Blue", "duration": "3:28"},
            ]
        },
        {
            "name": "Workout Mix",
            "tracks": [
                {"id": 4, "title": "Electric Rush", "artist": "High Energy", "duration": "3:15"},
                {"id": 5, "title": "Pulse", "artist": "Beat Drop", "duration": "3:52"},
                {"id": 6, "title": "Thunder Run", "artist": "Power Surge", "duration": "4:03"},
            ]
        },
        {
            "name": "Jazz Night",
            "tracks": [
                {"id": 7, "title": "Midnight Session", "artist": "Smooth Keys", "duration": "5:10"},
                {"id": 8, "title": "Blue Note", "artist": "Jazz Trio", "duration": "4:33"},
                {"id": 9, "title": "Cool Down", "artist": "Mellow Tones", "duration": "3:56"},
            ]
        },
    ]
    
    active_playlist, set_active_playlist = us(0)
    now_playing, set_now_playing = us(None)
    is_playing, set_is_playing = us(False)
    
    current_playlist = playlists[active_playlist]
    
    return div(cn="playlist-player",
        div(cn="sidebar",
            h3("Playlists"),
            *[div(cn=f"playlist-item{' active' if i == active_playlist else ''}",
                  oc=lambda: set_active_playlist(i),
                  div(cn="playlist-name", p["name"]),
                  div(cn="playlist-count", f"{len(p['tracks'])} tracks"),
              ) for i, p in enumerate(playlists)],
        ),
        div(cn="main-panel",
            h2(current_playlist["name"]),
            div(cn="track-list",
                *[div(cn=f"track-item{' now-playing' if t['id'] == now_playing else ''}",
                      oc=lambda: set_now_playing(t["id"]),
                      div(cn="track-title", t["title"]),
                      div(cn="track-artist", t["artist"]),
                      div(cn="track-duration", t["duration"]),
                  ) for t in current_playlist["tracks"]],
            ),
        ),
        div(cn="bottom-bar",
            div(cn="now-playing-info",
                f"Now playing: {next((t['title'] for t in current_playlist['tracks'] if t['id'] == now_playing), 'None')}" if now_playing else "No track selected",
            ),
            button(oc=lambda: set_is_playing(not is_playing),
                   "Pause" if is_playing else "Play"),
        ),
    )
```

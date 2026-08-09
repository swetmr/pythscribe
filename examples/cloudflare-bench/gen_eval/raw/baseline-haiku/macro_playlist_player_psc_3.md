<!-- exp=baseline-haiku task=macro_playlist_player cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.032549400000000006 -->
```python
"use client"
R*

@c
def PlaylistPlayer():
    playlists = [
        {
            "id": 1,
            "name": "Discover Weekly",
            "tracks": [
                {"id": 1, "title": "Midnight Rain", "artist": "Taylor Swift", "duration": "3:45"},
                {"id": 2, "title": "Blinding Lights", "artist": "The Weeknd", "duration": "3:20"},
                {"id": 3, "title": "Levitating", "artist": "Dua Lipa", "duration": "3:23"},
            ]
        },
        {
            "id": 2,
            "name": "Chill Vibes",
            "tracks": [
                {"id": 4, "title": "Good as Hell", "artist": "Lizzo", "duration": "2:58"},
                {"id": 5, "title": "Skinny Love", "artist": "Bon Iver", "duration": "3:58"},
                {"id": 6, "title": "Home", "artist": "Phillip Phillips", "duration": "3:52"},
            ]
        },
        {
            "id": 3,
            "name": "Workout Mix",
            "tracks": [
                {"id": 7, "title": "Eye of the Tiger", "artist": "Survivor", "duration": "4:09"},
                {"id": 8, "title": "Don't Stop Me Now", "artist": "Queen", "duration": "3:38"},
                {"id": 9, "title": "Thunderstruck", "artist": "AC/DC", "duration": "4:52"},
            ]
        }
    ]

    active_playlist_id, set_active_playlist_id = us(playlists[0]["id"])
    now_playing_track_id, set_now_playing_track_id = us(None)
    is_playing, set_is_playing = us(False)

    active_playlist = [p for p in playlists if p["id"] == active_playlist_id][0]
    
    now_playing_title = None
    for t in active_playlist["tracks"]:
        if t["id"] == now_playing_track_id:
            now_playing_title = t["title"]
            break
    
    def toggle_play():
        set_is_playing(not is_playing)

    return div(cn="playlist-player",
        div(cn="sidebar",
            h3("Playlists"),
            *[div(
                key=p["id"],
                cn=f"playlist-item {'active' if p['id'] == active_playlist_id else ''}",
                oc=lambda pid=p["id"]: set_active_playlist_id(pid),
                div(cn="playlist-name", p["name"]),
                div(cn="track-count", f"{len(p['tracks'])} tracks"),
            ) for p in playlists]
        ),
        div(cn="main-panel",
            h2(active_playlist["name"]),
            div(cn="tracks-list",
                *[div(
                    key=t["id"],
                    cn=f"track-item {'now-playing' if t['id'] == now_playing_track_id else ''}",
                    oc=lambda tid=t["id"]: set_now_playing_track_id(tid),
                    div(cn="track-info",
                        div(cn="track-title", t["title"]),
                        div(cn="track-artist", t["artist"]),
                    ),
                    div(cn="track-duration", t["duration"]),
                ) for t in active_playlist["tracks"]]
            ),
        ),
        div(cn="player-bar",
            div(cn="now-playing-info",
                div(cn="now-playing-title", now_playing_title or "No track selected"),
            ),
            button(
                cn="play-button",
                oc=toggle_play,
                "▶" if not is_playing else "⏸"
            ),
        ),
    )
```

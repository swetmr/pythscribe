<!-- exp=baseline-haiku task=macro_playlist_player cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.05520520000000001 -->
```python
"use client"

from pyths.react import component, use_state

@component
def PlaylistPlayer():
    playlists = [
        {
            "id": 1,
            "name": "Chill Vibes",
            "tracks": [
                {"id": 1, "title": "Dreamy Days", "artist": "Luna & Stars", "duration": "3:45"},
                {"id": 2, "title": "Peaceful Mind", "artist": "Calm Waves", "duration": "4:12"},
                {"id": 3, "title": "Sunset Glow", "artist": "Evening Light", "duration": "3:28"},
                {"id": 4, "title": "Starlit Night", "artist": "Luna & Stars", "duration": "5:15"},
            ]
        },
        {
            "id": 2,
            "name": "Workout Mix",
            "tracks": [
                {"id": 5, "title": "Energy Burst", "artist": "Pump It Up", "duration": "3:15"},
                {"id": 6, "title": "Push Harder", "artist": "Beat Makers", "duration": "4:45"},
                {"id": 7, "title": "Running Fast", "artist": "Speed Kings", "duration": "3:52"},
            ]
        },
        {
            "id": 3,
            "name": "Focus Session",
            "tracks": [
                {"id": 8, "title": "Deep Focus", "artist": "Ambient Master", "duration": "5:30"},
                {"id": 9, "title": "Zen Vibes", "artist": "Meditation Sounds", "duration": "4:00"},
                {"id": 10, "title": "Concentration", "artist": "Brain Boost", "duration": "6:15"},
                {"id": 11, "title": "Productive Mind", "artist": "Ambient Master", "duration": "4:45"},
            ]
        },
    ]
    
    active_playlist_id, set_active_playlist_id = use_state(1)
    now_playing_id, set_now_playing_id = use_state(None)
    is_playing, set_is_playing = use_state(False)
    
    active_playlist = None
    for p in playlists:
        if p["id"] == active_playlist_id:
            active_playlist = p
            break
    
    now_playing_track = None
    if active_playlist and now_playing_id:
        for t in active_playlist["tracks"]:
            if t["id"] == now_playing_id:
                now_playing_track = t
                break
    
    def on_select_playlist(pid):
        set_active_playlist_id(pid)
        set_now_playing_id(None)
    
    def on_select_track(tid):
        set_now_playing_id(tid)
        set_is_playing(True)
    
    def on_toggle_play():
        set_is_playing(not is_playing)
    
    return div(class_name="playlist-player",
        div(class_name="sidebar",
            h3("Playlists"),
            ul(*[
                li(
                    class_name=("playlist-item active" if p["id"] == active_playlist_id else "playlist-item"),
                    on_click=lambda pid=p["id"]: on_select_playlist(pid),
                    div(class_name="playlist-name", p["name"]),
                    div(class_name="track-count", f"{len(p['tracks'])} tracks")
                )
                for p in playlists
            ])
        ),
        
        div(class_name="main-panel",
            h2(active_playlist["name"] if active_playlist else "Select a Playlist"),
            ul(*[
                li(
                    class_name=("track-item now-playing" if t["id"] == now_playing_id else "track-item"),
                    on_click=lambda tid=t["id"]: on_select_track(tid),
                    div(class_name="track-title", t["title"]),
                    div(class_name="track-artist", t["artist"]),
                    div(class_name="track-duration", t["duration"])
                )
                for t in (active_playlist["tracks"] if active_playlist else [])
            ])
        ),
        
        div(class_name="bottom-bar",
            div(class_name="now-playing-info",
                now_playing_track["title"] if now_playing_track else "No track selected"
            ),
            button(
                class_name="play-pause-btn",
                on_click=lambda: on_toggle_play(),
                "Play" if not is_playing else "Pause"
            )
        )
    )
```

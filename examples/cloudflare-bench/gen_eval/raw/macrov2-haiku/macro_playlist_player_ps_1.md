<!-- exp=macrov2-haiku task=macro_playlist_player cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0506886 -->
```python
"use client"

from pyths.react import component, use_state

@component
def PlaylistPlayer():
    playlists = [
        {
            "id": "p1",
            "name": "Summer Vibes",
            "tracks": [
                {"id": "t1", "title": "Sunny Days", "artist": "The Bright Side", "duration": 180},
                {"id": "t2", "title": "Beach Life", "artist": "Ocean Sounds", "duration": 240},
                {"id": "t3", "title": "Golden Hour", "artist": "Sunset Collective", "duration": 210},
                {"id": "t4", "title": "Tropical Breeze", "artist": "Island Vibes", "duration": 195},
            ]
        },
        {
            "id": "p2",
            "name": "Workout Mix",
            "tracks": [
                {"id": "t5", "title": "Pump It Up", "artist": "Energy Boosters", "duration": 200},
                {"id": "t6", "title": "Going Harder", "artist": "Intensity Crew", "duration": 220},
                {"id": "t7", "title": "Keep Moving", "artist": "Momentum", "duration": 190},
                {"id": "t8", "title": "Push Through", "artist": "Motivation Squad", "duration": 210},
            ]
        },
        {
            "id": "p3",
            "name": "Chill Lofi",
            "tracks": [
                {"id": "t9", "title": "Ambient Dream", "artist": "Lofi Master", "duration": 240},
                {"id": "t10", "title": "Peaceful Nights", "artist": "Calm Vibes", "duration": 220},
                {"id": "t11", "title": "Study Sessions", "artist": "Focus Beats", "duration": 250},
                {"id": "t12", "title": "Late Night Thoughts", "artist": "Night Owl", "duration": 200},
            ]
        },
    ]

    active_playlist_id, set_active_playlist_id = use_state("p1")
    now_playing_id, set_now_playing_id = use_state("t1")
    is_playing, set_is_playing = use_state(False)

    active_playlist = None
    for p in playlists:
        if p["id"] == active_playlist_id:
            active_playlist = p
            break

    now_playing_track = None
    if active_playlist:
        for t in active_playlist["tracks"]:
            if t["id"] == now_playing_id:
                now_playing_track = t
                break

    def format_duration(seconds):
        minutes = seconds // 60
        secs = seconds % 60
        return f"{minutes}:{secs:02d}"

    def on_playlist_click(playlist_id):
        set_active_playlist_id(playlist_id)
        for p in playlists:
            if p["id"] == playlist_id:
                set_now_playing_id(p["tracks"][0]["id"])
                break

    def on_track_click(track_id):
        set_now_playing_id(track_id)
        set_is_playing(True)

    def toggle_play():
        set_is_playing(not is_playing)

    return div(class_name="playlist-player",
        div(class_name="sidebar",
            h2("Playlists"),
            *[div(
                class_name=f"playlist-item {'active' if p['id'] == active_playlist_id else ''}",
                on_click=lambda: on_playlist_click(p["id"]),
                div(class_name="playlist-name", p["name"]),
                div(class_name="track-count", f"{len(p['tracks'])} tracks"),
            ) for p in playlists]
        ),
        div(class_name="main-panel",
            h2(active_playlist["name"] if active_playlist else "Select a playlist"),
            div(class_name="tracklist",
                *[div(
                    class_name=f"track-item {'now-playing' if t['id'] == now_playing_id else ''}",
                    on_click=lambda: on_track_click(t["id"]),
                    div(class_name="track-title", t["title"]),
                    div(class_name="track-artist", t["artist"]),
                    div(class_name="track-duration", format_duration(t["duration"])),
                ) for t in (active_playlist["tracks"] if active_playlist else [])]
            ),
        ),
        div(class_name="player-bar",
            div(class_name="now-playing-info",
                div(class_name="now-playing-title", now_playing_track["title"] if now_playing_track else "No track selected"),
                div(class_name="now-playing-artist", now_playing_track["artist"] if now_playing_track else ""),
            ),
            button(
                class_name=f"play-button {'playing' if is_playing else 'paused'}",
                on_click=lambda: toggle_play(),
                "▶ Play" if not is_playing else "⏸ Pause"
            ),
        ),
    )
```

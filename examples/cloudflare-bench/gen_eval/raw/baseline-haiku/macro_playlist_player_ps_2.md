<!-- exp=baseline-haiku task=macro_playlist_player cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.025800199999999995 -->
```python
"use client"

from pyths.react import component, use_state

PLAYLISTS = [
    {
        "name": "Chill Vibes",
        "tracks": [
            {"title": "Moonlight", "artist": "Luna Sky", "duration": "3:42"},
            {"title": "Ocean Waves", "artist": "Water Sound", "duration": "4:15"},
            {"title": "Sunset Dreams", "artist": "Golden Hour", "duration": "3:58"},
            {"title": "Starry Night", "artist": "Night Owl", "duration": "4:33"},
        ]
    },
    {
        "name": "Workout Mix",
        "tracks": [
            {"title": "Energy Boost", "artist": "Thunder Road", "duration": "3:20"},
            {"title": "Heart Pumping", "artist": "Rhythm Kings", "duration": "3:45"},
            {"title": "Full Power", "artist": "Electric Pulse", "duration": "3:58"},
        ]
    },
    {
        "name": "Study Session",
        "tracks": [
            {"title": "Focus Time", "artist": "Calm Minds", "duration": "5:12"},
            {"title": "Concentration", "artist": "Zen Masters", "duration": "4:28"},
            {"title": "Productive Hours", "artist": "Flow State", "duration": "6:01"},
            {"title": "Brain Power", "artist": "Smart Frequency", "duration": "5:45"},
            {"title": "Deep Work", "artist": "Meditation Sound", "duration": "7:33"},
        ]
    },
]

@component
def PlaylistPlayer():
    active_playlist, set_active_playlist = use_state(0)
    now_playing, set_now_playing = use_state(None)
    is_playing, set_is_playing = use_state(False)

    current_playlist = PLAYLISTS[active_playlist]
    current_tracks = current_playlist["tracks"]
    
    now_playing_track = current_tracks[now_playing] if now_playing is not None and now_playing < len(current_tracks) else None

    return div(class_name="player-container",
        div(class_name="sidebar",
            h2("Playlists"),
            div(class_name="playlist-list",
                *[div(
                    key=i,
                    class_name=f"playlist-item {'active' if i == active_playlist else ''}",
                    on_click=lambda idx=i: set_active_playlist(idx),
                    div(class_name="playlist-name", PLAYLISTS[i]["name"]),
                    div(class_name="playlist-count", f"{len(PLAYLISTS[i]['tracks'])} tracks"),
                ) for i in range(len(PLAYLISTS))]
            )
        ),
        
        div(class_name="main-panel",
            h2(current_playlist["name"]),
            div(class_name="tracks-list",
                *[div(
                    key=i,
                    class_name=f"track-item {'now-playing' if i == now_playing else ''}",
                    on_click=lambda idx=i: set_now_playing(idx),
                    div(class_name="track-title", current_tracks[i]["title"]),
                    div(class_name="track-artist", current_tracks[i]["artist"]),
                    div(class_name="track-duration", current_tracks[i]["duration"]),
                ) for i in range(len(current_tracks))]
            )
        ),
        
        div(class_name="bottom-bar",
            div(class_name="now-playing-info",
                div(class_name="np-label", "Now Playing:"),
                div(class_name="np-title", now_playing_track["title"] if now_playing_track else "Select a track"),
            ),
            button(
                class_name="play-pause-btn",
                on_click=lambda: set_is_playing(not is_playing),
                "⏸ Pause" if is_playing else "▶ Play"
            )
        )
    )
```

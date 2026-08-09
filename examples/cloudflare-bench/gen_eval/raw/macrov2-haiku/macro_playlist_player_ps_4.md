<!-- exp=macrov2-haiku task=macro_playlist_player cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.010107399999999999 -->
I'll use the brainstorming skill to structure the component approach before writing the code.

[Skill tool invocation needed]

Using superpowers:brainstorming to design the PlaylistPlayer component structure.

## Approach

**Component structure:** three sections in a flex layout (sidebar | main | bottom). State: active playlist ID, now-playing track ID, and play/pause toggle. Fixture data: array of playlists, each with ID, name, and track array (track ID, title, artist, duration).

**Key trade-off:** storing now-playing as track ID vs. track object — using ID is simpler for highlighting and survives playlist changes.

Now building the component:

```python
"use client"
from pyths.react import component, use_state

@component
def PlaylistPlayer():
    playlists_data = [
        {
            "id": "p1",
            "name": "Chill Vibes",
            "tracks": [
                {"id": "t1", "title": "Moonlight", "artist": "Luna Mae", "duration": "3:45"},
                {"id": "t2", "title": "Ocean Waves", "artist": "Aqua Dreams", "duration": "4:12"},
                {"id": "t3", "title": "Sunset Glow", "artist": "Day Drift", "duration": "3:28"},
            ]
        },
        {
            "id": "p2",
            "name": "Workout Beats",
            "tracks": [
                {"id": "t4", "title": "Running High", "artist": "Pulse", "duration": "3:32"},
                {"id": "t5", "title": "Power Up", "artist": "Energy X", "duration": "3:55"},
                {"id": "t6", "title": "All In", "artist": "Drive", "duration": "4:01"},
            ]
        },
        {
            "id": "p3",
            "name": "Late Night Grooves",
            "tracks": [
                {"id": "t7", "title": "Neon Nights", "artist": "Synth Wave", "duration": "4:20"},
                {"id": "t8", "title": "City Lights", "artist": "Urban Echo", "duration": "3:48"},
                {"id": "t9", "title": "Midnight Cruise", "artist": "Night Rider", "duration": "4:35"},
            ]
        },
    ]

    active_playlist_id, set_active_playlist_id = use_state("p1")
    now_playing_id, set_now_playing_id = use_state("t1")
    is_playing, set_is_playing = use_state(True)

    active_playlist = next((p for p in playlists_data if p["id"] == active_playlist_id), playlists_data[0])

    def select_track(track_id):
        set_now_playing_id(track_id)
        set_is_playing(True)

    return div(class_name="playlist-player",
        div(class_name="sidebar",
            h3("Playlists"),
            *[div(
                class_name=f"playlist-item {'active' if p['id'] == active_playlist_id else ''}",
                on_click=lambda p_id=p["id"]: set_active_playlist_id(p_id),
                div(class_name="playlist-name", p["name"]),
                div(class_name="playlist-count", f"{len(p['tracks'])} tracks"),
            ) for p in playlists_data],
        ),
        div(class_name="main-panel",
            h2(active_playlist["name"]),
            div(class_name="tracks-list",
                *[div(
                    class_name=f"track-item {'now-playing' if t['id'] == now_playing_id else ''}",
                    on_click=lambda t_id=t["id"]: select_track(t_id),
                    div(class_name="track-title", t["title"]),
                    div(class_name="track-artist", t["artist"]),
                    div(class_name="track-duration", t["duration"]),
                ) for t in active_playlist["tracks"]],
            ),
        ),
        div(class_name="bottom-bar",
            div(class_name="now-playing-info",
                p(now_playing_id and next((t["title"] for p in playlists_data for t in p["tracks"] if t["id"] == now_playing_id), "No track")),
            ),
            button(
                class_name="play-pause-btn",
                on_click=lambda: set_is_playing(not is_playing),
                "▶ Play" if not is_playing else "⏸ Pause",
            ),
        ),
    )
```

Component built with sidebar playlist selection, main track list with now-playing highlighting, and bottom bar with play/pause toggle.

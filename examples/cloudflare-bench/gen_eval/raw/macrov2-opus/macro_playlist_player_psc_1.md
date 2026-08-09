<!-- exp=macrov2-opus task=macro_playlist_player cond=psc sample=1 model=claude-opus-4-8 cost_usd=0.090084 -->
```python
"use client"
R*

PLAYLISTS = [
    {"id": "p1", "name": "Chill Vibes", "tracks": [
        {"title": "Sunset Drive", "artist": "Lо-Fi Collective", "duration": "3:24"},
        {"title": "Ocean Breeze", "artist": "Calm Waters", "duration": "4:02"},
        {"title": "Night Sky", "artist": "Dreamscape", "duration": "2:58"},
    ]},
    {"id": "p2", "name": "Workout Mix", "tracks": [
        {"title": "Power Up", "artist": "Adrenaline", "duration": "3:11"},
        {"title": "Full Throttle", "artist": "Redline", "duration": "3:47"},
        {"title": "Beast Mode", "artist": "Iron Pulse", "duration": "4:20"},
        {"title": "Sprint", "artist": "Fast Lane", "duration": "2:33"},
    ]},
    {"id": "p3", "name": "Focus Flow", "tracks": [
        {"title": "Deep Work", "artist": "Ambient Mind", "duration": "5:10"},
        {"title": "Clarity", "artist": "Zen Loop", "duration": "4:44"},
        {"title": "Momentum", "artist": "Steady State", "duration": "3:36"},
    ]},
]


def find_playlist(pid):
    for p in PLAYLISTS:
        if p["id"] == pid:
            return p
    return PLAYLISTS[0]


@c
def PlaylistPlayer():
    active_id, set_active_id = us(PLAYLISTS[0]["id"])
    now_playing, set_now_playing = us(None)
    is_playing, set_is_playing = us(False)

    active = find_playlist(active_id)

    def select_playlist(pid):
        set_active_id(pid)

    def play_track(track):
        set_now_playing(track)
        set_is_playing(True)

    def toggle_play():
        if now_playing:
            set_is_playing(not is_playing)

    return div(cn="player",
        div(cn="sidebar",
            h2(cn="sidebar-title", "Playlists"),
            ul(cn="playlist-list",
                *[li(key=p["id"],
                     cn="playlist-item active" if p["id"] == active_id else "playlist-item",
                     oc=lambda: select_playlist(p["id"]),
                     span(cn="playlist-name", p["name"]),
                     span(cn="playlist-count", f"{len(p['tracks'])} tracks"))
                  for p in PLAYLISTS]),
        ),
        div(cn="main",
            h1(cn="main-title", active["name"]),
            ul(cn="track-list",
                *[li(key=i,
                     cn="track-item playing" if now_playing and t["title"] == now_playing["title"] else "track-item",
                     oc=lambda: play_track(t),
                     span(cn="track-title", t["title"]),
                     span(cn="track-artist", t["artist"]),
                     span(cn="track-duration", t["duration"]))
                  for i, t in enumerate(active["tracks"])]),
        ),
        div(cn="now-bar",
            span(cn="now-label",
                 f"Now Playing: {now_playing['title']}" if now_playing else "Nothing playing"),
            button(cn="play-toggle", oc=lambda: toggle_play(), dis=now_playing is None,
                   "Pause" if is_playing else "Play"),
        ),
    )
```

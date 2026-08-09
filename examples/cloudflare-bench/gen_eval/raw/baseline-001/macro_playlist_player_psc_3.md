<!-- exp=baseline-001 task=macro_playlist_player cond=psc sample=3 model=claude-opus-4-8 cost_usd=0.045595000000000004 -->
```python
"use client"
R*

PLAYLISTS = [
    {"id": "p1", "name": "Chill Vibes", "tracks": [
        {"title": "Sunset Drive", "artist": "Nova", "duration": "3:42"},
        {"title": "Ocean Eyes", "artist": "Marlo", "duration": "4:05"},
        {"title": "Slow Motion", "artist": "Kaya", "duration": "2:58"},
    ]},
    {"id": "p2", "name": "Workout Mix", "tracks": [
        {"title": "Adrenaline", "artist": "Volt", "duration": "3:20"},
        {"title": "Push It", "artist": "Rex", "duration": "2:47"},
        {"title": "Overdrive", "artist": "Nyx", "duration": "3:11"},
        {"title": "Ignite", "artist": "Ash", "duration": "3:55"},
    ]},
    {"id": "p3", "name": "Focus Flow", "tracks": [
        {"title": "Deep Work", "artist": "Loom", "duration": "5:12"},
        {"title": "Quiet Mind", "artist": "Sable", "duration": "4:33"},
        {"title": "Steady", "artist": "Onyx", "duration": "3:47"},
    ]},
]


def find_playlist(pid):
    for pl in PLAYLISTS:
        if pl["id"] == pid:
            return pl
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

    def same_track(t):
        return now_playing is not None and t["title"] == now_playing["title"] and t["artist"] == now_playing["artist"]

    return div(cn="player",
        div(cn="player-body",
            aside(cn="sidebar",
                h2(cn="sidebar-title", "Playlists"),
                ul(cn="playlist-list",
                    *[li(key=pl["id"],
                        cn="playlist-item active" if pl["id"] == active_id else "playlist-item",
                        oc=lambda pid=pl["id"]: select_playlist(pid),
                        span(cn="playlist-name", pl["name"]),
                        span(cn="playlist-count", f"{len(pl['tracks'])} tracks"),
                      ) for pl in PLAYLISTS]),
            ),
            main(cn="main-panel",
                h2(cn="panel-title", active["name"]),
                ul(cn="track-list",
                    *[li(key=f"{active['id']}-{i}",
                        cn="track-item playing" if same_track(t) else "track-item",
                        oc=lambda tr=t: play_track(tr),
                        span(cn="track-title", t["title"]),
                        span(cn="track-artist", t["artist"]),
                        span(cn="track-duration", t["duration"]),
                      ) for i, t in enumerate(active["tracks"])]),
            ),
        ),
        div(cn="now-bar",
            div(cn="now-info",
                span(cn="now-label", "Now Playing"),
                span(cn="now-title",
                     now_playing["title"] if now_playing else "Nothing playing"),
                now_playing and span(cn="now-artist", now_playing["artist"]),
            ),
            button(cn="play-toggle",
                   dis=now_playing is None,
                   oc=lambda: toggle_play(),
                   "Pause" if (is_playing and now_playing) else "Play"),
        ),
    )
```

<!-- exp=macrov2-sonnet task=macro_tweet_composer cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.0456306 -->
```python
"use client"

from pyths.react import component, use_state

INITIAL_TWEETS = [
    {"id": 1, "author": "ada_lovelace", "text": "Just shipped the first loop! #analyticalengine", "likes": 42},
    {"id": 2, "author": "grace_hopper", "text": "Found an actual bug in the relay today.", "likes": 128},
    {"id": 3, "author": "alan_turing", "text": "Can machines think? Working on a test for that.", "likes": 256},
]

MAX_LEN = 280

@component
def TweetFeed():
    tweets, set_tweets = use_state(INITIAL_TWEETS)
    draft, set_draft = use_state("")
    next_id, set_next_id = use_state(len(INITIAL_TWEETS) + 1)

    remaining = MAX_LEN - len(draft)
    can_post = len(draft.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new_tweet = {"id": next_id, "author": "you", "text": draft.strip(), "likes": 0}
        set_tweets([new_tweet, *tweets])
        set_draft("")
        set_next_id(next_id + 1)

    def like(tweet_id):
        updated = [
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ]
        set_tweets(updated)

    counter_class = "tweet-counter over-limit" if remaining < 0 else "tweet-counter"

    return div(class_name="tweet-feed",
        div(class_name="tweet-compose",
            textarea(
                class_name="tweet-input",
                value=draft,
                on_change=lambda e: set_draft(e.target.value),
                placeholder="What's happening?",
            ),
            div(class_name="tweet-compose-footer",
                span(class_name=counter_class, f"{remaining}"),
                button(
                    class_name="tweet-post-btn",
                    disabled=not can_post,
                    on_click=lambda: post(),
                    "Post",
                ),
            ),
        ),
        ul(class_name="tweet-list",
            *[li(key=t["id"], class_name="tweet-item",
                div(class_name="tweet-author", t["author"]),
                p(class_name="tweet-text", t["text"]),
                div(class_name="tweet-actions",
                    button(
                        class_name="tweet-like-btn",
                        on_click=lambda: like(t["id"]),
                        f"♥ {t['likes']}",
                    ),
                ),
            ) for t in tweets],
        ),
    )
```

<!-- exp=macrov2-sonnet task=macro_tweet_composer cond=ps sample=3 model=claude-sonnet-5 cost_usd=0.024132 -->
```python
"use client"

from pyths.react import component, use_state

INITIAL_TWEETS = [
    {"id": 3, "author": "ada_lovelace", "text": "The Analytical Engine has no pretensions whatever to originate anything.", "likes": 42},
    {"id": 2, "author": "grace_hopper", "text": "The most dangerous phrase in the language is 'we've always done it this way.'", "likes": 108},
    {"id": 1, "author": "alan_turing", "text": "Machines take me by surprise with great frequency.", "likes": 77},
]

MAX_CHARS = 280


@component
def TweetFeed():
    tweets, set_tweets = use_state(INITIAL_TWEETS)
    draft, set_draft = use_state("")
    next_id, set_next_id = use_state(4)

    remaining = MAX_CHARS - len(draft)
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

    counter_class = "char-counter over-limit" if remaining < 0 else "char-counter"

    return div(class_name="tweet-feed",
        div(class_name="compose",
            textarea(
                class_name="compose-input",
                value=draft,
                on_change=lambda e: set_draft(e.target.value),
                placeholder="What's happening?",
            ),
            div(class_name="compose-footer",
                span(class_name=counter_class, f"{remaining}"),
                button(
                    class_name="post-button",
                    disabled=not can_post,
                    on_click=lambda: post(),
                    "Post",
                ),
            ),
        ),
        ul(class_name="tweet-list",
            *[
                li(key=t["id"], class_name="tweet",
                    div(class_name="tweet-author", f"@{t['author']}"),
                    p(class_name="tweet-text", t["text"]),
                    button(
                        class_name="like-button",
                        on_click=lambda tid=t["id"]: like(tid) if False else like(t["id"]),
                        f"♥ {t['likes']}",
                    ),
                )
                for t in tweets
            ]
        ),
    )
```

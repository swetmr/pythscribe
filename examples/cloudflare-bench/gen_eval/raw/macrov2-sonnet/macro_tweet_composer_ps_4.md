<!-- exp=macrov2-sonnet task=macro_tweet_composer cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.025016999999999998 -->
```python
"use client"

from pyths.react import component, use_state

INITIAL_TWEETS = [
    {"id": 1, "author": "ada_lovelace", "text": "Just finished debugging the analytical engine.", "likes": 12},
    {"id": 2, "author": "grace_hopper", "text": "Found an actual bug in the relay today.", "likes": 34},
    {"id": 3, "author": "alan_turing", "text": "Can machines think? Working on a test for that.", "likes": 58},
]

MAX_LENGTH = 280


@component
def TweetFeed():
    tweets, set_tweets = use_state(INITIAL_TWEETS)
    draft, set_draft = use_state("")

    remaining = MAX_LENGTH - len(draft)
    can_post = len(draft.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new_tweet = {
            "id": len(tweets) + 1 if len(tweets) == 0 else max([t["id"] for t in tweets]) + 1,
            "author": "you",
            "text": draft.strip(),
            "likes": 0,
        }
        set_tweets([new_tweet, *tweets])
        set_draft("")

    def like(tweet_id):
        updated = [
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ]
        set_tweets(updated)

    counter_class = "char-counter over-limit" if remaining < 0 else "char-counter"

    return div(class_name="tweet-feed",
        div(class_name="compose-box",
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
                li(class_name="tweet", key=t["id"],
                    div(class_name="tweet-header",
                        span(class_name="tweet-author", f"@{t['author']}"),
                    ),
                    p(class_name="tweet-text", t["text"]),
                    div(class_name="tweet-actions",
                        button(
                            class_name="like-button",
                            on_click=lambda tid=t["id"]: like(tid) if False else None,
                        ),
                    ),
                )
                for t in tweets
            ]
        ),
    )
```

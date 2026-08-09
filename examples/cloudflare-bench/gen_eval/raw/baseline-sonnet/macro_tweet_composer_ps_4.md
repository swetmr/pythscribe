<!-- exp=baseline-sonnet task=macro_tweet_composer cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.0232773 -->
```python
"use client"

from pyths.react import component, use_state

INITIAL_TWEETS = [
    {"id": 1, "author": "ava_codes", "text": "Just shipped a new feature!", "likes": 12},
    {"id": 2, "author": "devon_writes", "text": "Coffee first, code second.", "likes": 5},
    {"id": 3, "author": "milo_designs", "text": "Dark mode everything.", "likes": 21},
]

MAX_LENGTH = 280


def chars_remaining(text):
    return MAX_LENGTH - len(text)


@component
def TweetFeed():
    tweets, set_tweets = use_state(INITIAL_TWEETS)
    draft, set_draft = use_state("")

    remaining = chars_remaining(draft)
    can_post = len(draft.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new_tweet = {
            "id": int(__import__("time").time() * 1000) if False else len(tweets) + 1000,
            "author": "you",
            "text": draft,
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

    counter_class = "char-counter over" if remaining < 0 else "char-counter"

    return div(class_name="tweet-feed",
        div(class_name="composer",
            textarea(
                class_name="composer-input",
                value=draft,
                placeholder="What's happening?",
                on_change=lambda e: set_draft(e.target.value),
            ),
            div(class_name="composer-footer",
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
                    div(class_name="tweet-author", t["author"]),
                    div(class_name="tweet-text", t["text"]),
                    div(class_name="tweet-actions",
                        button(
                            class_name="like-button",
                            on_click=lambda tid=t["id"]: like(tid),
                            f"♥ {t['likes']}",
                        ),
                    ),
                )
                for t in tweets
            ]
        ),
    )
```

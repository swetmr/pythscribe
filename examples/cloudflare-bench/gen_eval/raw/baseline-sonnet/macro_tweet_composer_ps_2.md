<!-- exp=baseline-sonnet task=macro_tweet_composer cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.024777300000000002 -->
```python
"use client"

from pyths.react import component, use_state

SEED_TWEETS = [
    {"id": 3, "author": "@nova", "text": "Shipped a tiny compiler today. Feels great.", "likes": 12},
    {"id": 2, "author": "@pixel", "text": "Coffee first, code second.", "likes": 4},
    {"id": 1, "author": "@echo", "text": "Refactoring is just cleaning with extra steps.", "likes": 27},
]

MAX_CHARS = 280

def next_id(tweets):
    if len(tweets) == 0:
        return 1
    return max(t["id"] for t in tweets) + 1

@component
def TweetFeed():
    tweets, set_tweets = use_state(SEED_TWEETS)
    draft, set_draft = use_state("")

    remaining = MAX_CHARS - len(draft)
    can_post = len(draft.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new_tweet = {
            "id": next_id(tweets),
            "author": "@you",
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
                    p(class_name="tweet-text", t["text"]),
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

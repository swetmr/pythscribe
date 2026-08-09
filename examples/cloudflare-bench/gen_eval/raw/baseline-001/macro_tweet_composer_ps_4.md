<!-- exp=baseline-001 task=macro_tweet_composer cond=ps sample=4 model=claude-opus-4-8 cost_usd=0.032941 -->
```python
"use client"

from pyths.react import component, use_state

CHAR_LIMIT = 280

SEED_TWEETS = [
    {"id": 1, "author": "@ada", "text": "Just shipped a new compiler pass. Feeling good!", "likes": 12},
    {"id": 2, "author": "@grace", "text": "Reminder: test your edge cases before you ship.", "likes": 34},
    {"id": 3, "author": "@linus", "text": "Talk is cheap. Show me the code.", "likes": 128},
]


def next_id(tweets):
    return (max([t["id"] for t in tweets]) + 1) if tweets else 1


@component
def TweetFeed():
    tweets, set_tweets = use_state(SEED_TWEETS)
    draft, set_draft = use_state("")

    remaining = CHAR_LIMIT - len(draft)
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
        set_tweets([
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ])

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
                span(class_name=counter_class, str(remaining)),
                button(
                    class_name="post-button",
                    on_click=lambda: post(),
                    disabled=not can_post,
                    "Tweet",
                ),
            ),
        ),
        ul(class_name="tweet-list",
            *[li(key=t["id"], class_name="tweet",
                div(class_name="tweet-author", t["author"]),
                div(class_name="tweet-text", t["text"]),
                button(
                    class_name="like-button",
                    on_click=lambda tid=t["id"]: like(tid),
                    f"♥ {t['likes']}",
                ),
            ) for t in tweets],
        ),
    )
```

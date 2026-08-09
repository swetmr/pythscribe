<!-- exp=baseline-sonnet task=macro_tweet_composer cond=ps sample=5 model=claude-sonnet-5 cost_usd=0.0431073 -->
```python
"use client"

from pyths.react import component, use_state

MAX_LENGTH = 280

INITIAL_TWEETS = [
    {"id": 1, "author": "ada", "text": "Hello, world!", "likes": 3},
    {"id": 2, "author": "grace", "text": "Compilers are fun.", "likes": 5},
    {"id": 3, "author": "linus", "text": "Just a hobby, won't be big.", "likes": 12},
]

def next_id(tweets):
    if not tweets:
        return 1
    return max(t["id"] for t in tweets) + 1

@component
def TweetFeed():
    tweets, set_tweets = use_state(INITIAL_TWEETS)
    draft, set_draft = use_state("")

    remaining = MAX_LENGTH - len(draft)
    is_over = remaining < 0
    can_post = len(draft.strip()) > 0 and not is_over

    def post():
        if not can_post:
            return
        new_tweet = {
            "id": next_id(tweets),
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

    counter_class = "char-counter over-limit" if is_over else "char-counter"

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
        div(class_name="tweet-list",
            *[
                div(key=t["id"], class_name="tweet",
                    div(class_name="tweet-author", t["author"]),
                    div(class_name="tweet-text", t["text"]),
                    button(
                        class_name="like-button",
                        on_click=lambda tid=t["id"]: like(tid),
                        f"\u2665 {t['likes']}",
                    ),
                )
                for t in tweets
            ]
        ),
    )
```

<!-- exp=macrov2-opus task=macro_tweet_composer cond=ps sample=1 model=claude-opus-4-8 cost_usd=0.067034 -->
```python
"use client"

from pyths.react import component, use_state


def remaining_chars(text, limit=280):
    return limit - len(text)


@component
def TweetFeed():
    tweets, set_tweets = use_state([
        {"id": 1, "author": "ada", "text": "Just shipped a new compiler pass!", "likes": 12},
        {"id": 2, "author": "grace", "text": "Debugging is twice as hard as writing the code.", "likes": 34},
        {"id": 3, "author": "linus", "text": "Talk is cheap. Show me the code.", "likes": 88},
    ])
    draft, set_draft = use_state("")

    limit = 280
    remaining = remaining_chars(draft, limit)
    over_limit = remaining < 0
    is_empty = len(draft.strip()) == 0
    can_post = not is_empty and not over_limit

    def post():
        if not can_post:
            return
        next_id = max([t["id"] for t in tweets], default=0) + 1
        new_tweet = {"id": next_id, "author": "you", "text": draft.strip(), "likes": 0}
        set_tweets([new_tweet, *tweets])
        set_draft("")

    def like(tweet_id):
        set_tweets([
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ])

    counter_class = "char-counter over" if over_limit else "char-counter"

    return div(class_name="tweet-feed",
        h2("Home"),
        div(class_name="composer",
            textarea(
                class_name="composer-input",
                value=draft,
                on_change=lambda e: set_draft(e.target.value),
                placeholder="What's happening?",
            ),
            div(class_name="composer-actions",
                span(class_name=counter_class, f"{remaining}"),
                button(
                    class_name="post-button",
                    on_click=lambda: post(),
                    disabled=not can_post,
                    "Post",
                ),
            ),
        ),
        ul(class_name="tweet-list",
            *[li(key=t["id"], class_name="tweet",
                div(class_name="tweet-author", f"@{t['author']}"),
                p(class_name="tweet-text", t["text"]),
                button(
                    class_name="like-button",
                    on_click=lambda: like(t["id"]),
                    f"♥ {t['likes']}",
                ),
            ) for t in tweets],
        ),
    )
```

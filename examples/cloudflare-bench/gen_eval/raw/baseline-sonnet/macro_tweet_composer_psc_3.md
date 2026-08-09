<!-- exp=baseline-sonnet task=macro_tweet_composer cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.0562263 -->
```python
"use client"
R*

SEED = [
    {"id": 1, "author": "ada", "text": "Shipped a new compiler pass today.", "likes": 3},
    {"id": 2, "author": "grace", "text": "Debugging is twice as hard as writing code.", "likes": 12},
    {"id": 3, "author": "linus", "text": "Talk is cheap. Show me the code.", "likes": 47},
]

@c
def TweetFeed():
    tweets, set_tweets = us(SEED)
    draft, set_draft = us("")
    next_id, set_next_id = us(4)

    remaining = 280 - len(draft)
    can_post = len(draft.strip()) > 0 and remaining >= 0

    def post():
        if can_post:
            set_tweets([{"id": next_id, "author": "you", "text": draft, "likes": 0}, *tweets])
            set_draft("")
            set_next_id(next_id + 1)

    def like(tweet_id):
        set_tweets([{**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t for t in tweets])

    counter_class = "char-counter over" if remaining < 0 else "char-counter"

    return div(cn="tweet-feed",
        div(cn="composer",
            textarea(cn="composer-input", value=draft,
                     oh=lambda e: set_draft(e.target.value),
                     ph="What's happening?", max_length=280),
            div(cn="composer-footer",
                span(cn=counter_class, f"{remaining}"),
                button(cn="post-button", oc=lambda: post(), dis=not can_post, "Post"),
            ),
        ),
        ul(cn="tweet-list", *[
            li(key=t["id"], cn="tweet",
                div(cn="tweet-author", t["author"]),
                div(cn="tweet-text", t["text"]),
                button(cn="like-button", oc=lambda t=t: like(t["id"]), f"♥ {t['likes']}"),
            ) for t in tweets
        ]),
    )
```

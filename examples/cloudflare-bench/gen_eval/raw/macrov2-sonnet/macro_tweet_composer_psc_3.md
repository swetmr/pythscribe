<!-- exp=macrov2-sonnet task=macro_tweet_composer cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.022398599999999998 -->
```python
"use client"
R*

@c
def TweetFeed():
    tweets, set_tweets = us([
        {"id": 1, "author": "ada", "text": "Hello, world!", "likes": 3},
        {"id": 2, "author": "grace", "text": "Compilers are magic.", "likes": 5},
        {"id": 3, "author": "linus", "text": "Just push to main.", "likes": 8},
    ])
    draft, set_draft = us("")

    remaining = 280 - len(draft)
    can_post = len(draft.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new_tweet = {
            "id": max([t["id"] for t in tweets], default=0) + 1,
            "author": "you",
            "text": draft.strip(),
            "likes": 0,
        }
        set_tweets([new_tweet, *tweets])
        set_draft("")

    def like(tweet_id):
        set_tweets([{**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t for t in tweets])

    counter_class = "char-counter over" if remaining < 0 else "char-counter"

    return div(cn="tweet-feed",
        div(cn="composer",
            textarea(
                cn="composer-input",
                ph="What's happening?",
                value=draft,
                oh=lambda e: set_draft(e.target.value),
            ),
            div(cn="composer-footer",
                span(cn=counter_class, f"{remaining}"),
                button(oc=lambda: post(), dis=not can_post, "Post"),
            ),
        ),
        ul(cn="tweet-list", *[
            li(key=t["id"], cn="tweet",
                div(cn="tweet-author", t["author"]),
                div(cn="tweet-text", t["text"]),
                button(cn="like-button", oc=lambda: like(t["id"]), f"♥ {t['likes']}"),
            )
            for t in tweets
        ]),
    )
```

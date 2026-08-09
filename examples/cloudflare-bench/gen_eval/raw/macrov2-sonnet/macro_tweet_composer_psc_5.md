<!-- exp=macrov2-sonnet task=macro_tweet_composer cond=psc sample=5 model=claude-sonnet-5 cost_usd=0.022848599999999997 -->
```python
"use client"
R*

@c
def TweetFeed():
    seed = [
        {"id": 1, "author": "Ada Lovelace", "text": "The Analytical Engine weaves algebraic patterns.", "likes": 12},
        {"id": 2, "author": "Alan Turing", "text": "Can machines think? Let's find out.", "likes": 8},
        {"id": 3, "author": "Grace Hopper", "text": "It's easier to ask forgiveness than permission.", "likes": 20},
    ]

    tweets, set_tweets = us(seed)
    draft, set_draft = us("")

    remaining = 280 - len(draft)
    can_post = len(draft.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new_tweet = {"id": len(tweets) + 1000, "author": "You", "text": draft.strip(), "likes": 0}
        set_tweets([new_tweet, *tweets])
        set_draft("")

    def like(id):
        updated = [
            {**t, "likes": t["likes"] + 1} if t["id"] == id else t
            for t in tweets
        ]
        set_tweets(updated)

    return div(cn="tweet-feed",
        div(cn="composer",
            textarea(
                cn="composer-input",
                value=draft,
                oh=lambda e: set_draft(e.target.value),
                ph="What's happening?",
            ),
            div(cn="composer-footer",
                span(cn="char-counter" if remaining >= 0 else "char-counter over-limit",
                     f"{remaining}"),
                button(oc=lambda: post(), dis=not can_post, "Post"),
            ),
        ),
        ul(cn="tweet-list", *[
            li(key=t["id"], cn="tweet",
                div(cn="tweet-author", t["author"]),
                p(cn="tweet-text", t["text"]),
                button(cn="like-button", oc=lambda tid=t["id"]: like(tid),
                       f"♥ {t['likes']}"),
            )
            for t in tweets
        ]),
    )
```

use crate::quicksink::{make_sink, Action};
use async_std::{io, task};
use futures::{channel::mpsc, prelude::*};

#[test]
fn smoke_test() {
    task::block_on(async {
        let sink = make_sink(io::stdout(), |mut stdout, action| async move {
            match action {
                Action::Send(x) => stdout.write_all(x).await?,
                Action::Flush => stdout.flush().await?,
                Action::Close => stdout.close().await?,
            }
            Ok::<_, io::Error>(stdout)
        });

        let values = vec![Ok(&b"hello\n"[..]), Ok(&b"world\n"[..])];
        assert!(stream::iter(values).forward(sink).await.is_ok())
    })
}

#[test]
fn replay() {
    task::block_on(async {
        let (tx, rx) = mpsc::channel(5);

        let sink = make_sink(tx, |mut tx, action| async move {
            tx.send(action.clone()).await?;
            if action == Action::Close {
                tx.close().await?
            }
            Ok::<_, mpsc::SendError>(tx)
        });

        futures::pin_mut!(sink);

        let expected = [
            Action::Send("hello\n"),
            Action::Flush,
            Action::Send("world\n"),
            Action::Flush,
            Action::Close,
        ];

        for &item in &["hello\n", "world\n"] {
            sink.send(item).await.unwrap()
        }

        sink.close().await.unwrap();

        let actual = rx.collect::<Vec<_>>().await;

        assert_eq!(&expected[..], &actual[..])
    });
}

#[test]
fn error_does_not_panic() {
    task::block_on(async {
        let sink = make_sink(io::stdout(), |mut _stdout, _action| async move {
            Err(io::Error::other("oh no"))
        });

        futures::pin_mut!(sink);

        let result = sink.send("hello").await;
        match result {
            Err(crate::quicksink::Error::Send(e)) => {
                assert_eq!(e.kind(), io::ErrorKind::Other);
                assert_eq!(e.to_string(), "oh no")
            }
            _ => panic!("unexpected result: {:?}", result),
        };

        // Call send again, expect not to panic.
        let result = sink.send("hello").await;
        match result {
            Err(crate::quicksink::Error::Closed) => {}
            _ => panic!("unexpected result: {:?}", result),
        };
    })
}

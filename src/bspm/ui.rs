use std::collections::HashMap;
use std::cmp::min;
use anyhow::anyhow;
use smol::Task;
use async_std::io::{stdin,stdout};
use async_std::sync::{channel,Sender,Receiver,Mutex,RecvError};
use async_std::prelude::*;
use progress_string::*;
use termion::{color,cursor,style};
use once_cell::sync::Lazy;

static CONTEXT: Lazy<Mutex<Context>> = Lazy::new(|| {
    let ui = UI::new();
    let result = ui.context(String::new());
    ui.render().detach();
    Mutex::new(result)
});

pub async fn context(prefix: &str, name: &str) -> Context {
    let mut result = CONTEXT.lock().await.clone();
    result.name(format!("{} {}", prefix, name));
    result
}

pub struct UI {
    stdout_send: Sender<(String,Message)>,
    stdout_recv: Receiver<(String,Message)>,
    stdin_send: Sender<String>,
    stdin_recv: Receiver<String>,
}

pub enum Message {
    Notification(String),
    Progress((u64,u64,String)),
    Question(String),
    Quit(Sender<()>),
}

impl UI {
    fn new() -> Self {
        let (stdin_send, stdin_recv) = channel(1);
        let (stdout_send, stdout_recv) = channel(1);
        UI {
            stdin_send,
            stdin_recv,
            stdout_send,
            stdout_recv,
        }
    }

    fn context(&self, name: String) -> Context {
        let stdin = self.stdin_recv.clone();
        let stdout = self.stdout_send.clone();
        Context {
            name,
            stdout,
            stdin,
        }
    }

    async fn read_line(&self, msg: &str) -> anyhow::Result<()> {
        println!("{}", msg);
        let mut stdin = stdin();
        let mut answer_input = String::new();
        loop {
            let mut buffer = [0; 1];
            stdin.read(&mut buffer).await?;
            let chr = std::str::from_utf8(&buffer)?;
            if chr == "\n" {
                self.stdin_send.send(answer_input).await;
                break
            }
            answer_input += chr;
        }
        stdout().flush().await?;
        Ok(())
    }

    async fn draw(&self, screen: String) -> anyhow::Result<(), std::io::Error> {
        let mut outlock = stdout();
        outlock.write_all(screen.as_bytes()).await?;
        outlock.flush().await
    }

    async fn mainloop(self) -> anyhow::Result<()> {
        // Init
        let mut messages: Vec<String> = Vec::new();
        let mut bars: HashMap<String,Bar> = HashMap::new();
        let cls = format!(
            "{}{}{}🔦 blindspot package manger{}", 
            termion::clear::All, 
            cursor::Goto(1, 1),
            style::Bold,
            style::Reset,
        );

        while let Ok((context, msg)) = self.stdout_recv.recv().await {
            // Update
            let (t_x, t_y) = termion::terminal_size()?;
            match msg {
                Message::Quit(tx) => tx.send(()).await,
                Message::Notification(msg) => messages.push(fmt_msg(context, msg)),
                Message::Question(msg) => self.read_line(&fmt_msg(context, msg)).await?,
                Message::Progress(p) => {
                    if let Some(pbar) = bars.get_mut(&context) {
                        pbar.replace(p.0 as usize);
                    } else {
                        let b = get_bar(p.0 as usize, p.1 as usize, t_x as usize, &p.2);
                        bars.insert(p.2, b);
                    }
                }
            }

            // Draw
            let mut screen = cls.clone();
            let n_msg = messages.len();
            let max_msg = t_y as usize - (bars.len() + 2);
            let offset = n_msg - min(n_msg, max_msg);
            for (i, msg) in messages[offset..].iter().enumerate() {
                screen += &format!("{}{}", cursor::Goto(1, (bars.len() + i + 2) as u16), &msg);
            }
            for (i, pbar) in bars.iter().enumerate() {
                screen += &fmt_bar(i + 1, pbar.0, pbar.1);
            }
            screen += &format!("{}", cursor::Goto(1, min(t_y, (bars.len() + messages.len() + 2) as u16)));
            self.draw(screen).await?;
        }
        Err(anyhow!("UI failed to receive next message"))
    }

    fn render(self) -> Task<()> {
        Task::spawn(async move {
            self.mainloop()
                .await
                .expect("UI render failure");
        })
    }
}

fn fmt_msg(context: String, message: String) -> String {
    format!(
        "{}{}{}{} {}",
        color::Fg(termion::color::LightCyan),
        style::Bold,
        context,
        style::Reset,
        message.trim()
    )
}

fn fmt_bar(nth: usize, msg: &str, pbar: &Bar) -> String {
    format!(
        "{}🚛 {}{}{} {}{}{}kb{}",
        cursor::Goto(1, nth as u16 + 1),
        style::Italic,
        msg[..min(msg.len(), 64)].to_string(),
        style::Reset,
        style::Bold,
        color::Fg(termion::color::Blue),
        pbar.to_string(),
        style::Reset,
    )
}

fn get_bar(current: usize, total: usize, max_width: usize, msg: &str) -> Bar {
    let mut result = progress_string::BarBuilder::new()
        .total(total)
        .include_percent()
        .include_numbers()
        .empty_char(' ')
        .full_char('=')
        .width(max_width - min(msg[..min(msg.len(), 64)].len(), max_width - 30) - 30)
        .get_bar();
    result.replace(current);
    result
}

#[derive(Clone)]
pub struct Context {
    name: String,
    stdin: Receiver<String>,
    stdout: Sender<(String,Message)>,
}

impl Context {
    fn name(&mut self, new: String) {
        self.name = new
    }

    pub async fn notify(&self, msg: &str) {
        self.send(
            Message::Notification(msg.to_string())
        ).await
    }

    pub async fn quit(&self) -> anyhow::Result<(), RecvError>{
        let (tx, rx) = channel(1);
        self.send(Message::Quit(tx)).await;
        rx.recv().await
    }

    pub async fn ask(&self, msg: &str) -> anyhow::Result<String, RecvError> {
        self.send(Message::Question(msg.to_string())).await;
        self.stdin.recv().await
    }

    pub async fn ask_number(&self, min: usize, max: usize, msg: &str) -> anyhow::Result<usize> {
        while let Ok(input) = self.ask(msg).await {
            let line = input.trim();
            if let Ok(x) = line.parse::<usize>() {
                if x >= min && x < max {
                    return Ok(x)
                }
            }
            self.notify(&format!("Invalid input: {:?}", &line)).await;
        }
        Err(anyhow!("Failed to receive from STDIN"))
    }

    pub async fn progress(&self, current: u64, total: u64, msg: &str) {
        self.send(
            Message::Progress((current,total,msg.to_string()))
        ).await
    }

    async fn send(&self, msg: Message) {
        self.stdout.send((self.name.clone(), msg)).await
    }
}

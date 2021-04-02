use std::{collections::HashMap, sync::mpsc::{self, Receiver, Sender}, thread, time::{self, Duration}};

use super::manager::TaskMsg;

// recv cmd, set up timer.
// ret a token, in case that the manager wants to cancel it.
// send task to the manager queue.

type TimeoutCallback = TaskMsg;
pub enum TimerCmd {
    New(time::Instant, time::Duration, TimeoutCallback),
    Cancel(TimerToken),

}
struct TimerEntry {
    instant: time::Instant,
    time_lim: time::Duration,
    callback: TimeoutCallback,
}

pub type TimerToken = u32;

pub struct Timer {
    timer_list: HashMap<TimerToken, TimerEntry>,
    cmd_recv: Receiver<TimerCmd>,
    token_send: Sender<TimerToken>,
    task_send: Sender<TaskMsg>,
    token_base: u32,
}

impl Timer {
    pub fn new (task_send: Sender<TaskMsg>) -> (Self, Sender<TimerCmd>, Receiver<TimerToken>) {
        let (cmd_send, cmd_recv) = mpsc::channel::<TimerCmd>();
        let (token_send, token_recv) = mpsc::channel::<TimerToken>();
        (
            Timer{
                timer_list: HashMap::new(), cmd_recv, token_send, task_send, token_base: 0
            },
            cmd_send,
            token_recv
        )
    }

    pub fn start (&mut self) {
        loop{
            self.check_timers();
            thread::sleep(Duration::from_micros(10)); // sleep 10 us to slow down.

            // if new cmd comes
            if let Ok(t_cmd) = self.cmd_recv.try_recv() {
                // parse the cmd
                match t_cmd {
                    TimerCmd::New(instant, duration, callback) => {
                        let new_token = self.token_base;
                        self.token_base += 1;
                        self.timer_list.insert(new_token, TimerEntry{instant, time_lim: duration, callback});
                        self.token_send.send(new_token).unwrap();
                    },
                    TimerCmd::Cancel(token) => {
                        // remove the timer entry
                        self.timer_list.remove(&token);
                    }
                    _ => {}
                }
            }
        }
    }

    // routine check for the timer list
    fn check_timers (&mut self) {
        let now = time::Instant::now();
        let mut time_outs = Vec::<TimerToken>::new();
        for (t_token, t_entry) in &mut self.timer_list {
            if now < t_entry.instant + t_entry.time_lim {
                // we got a time out
                time_outs.push(t_token.to_owned());
                // send callback
                self.task_send.send(t_entry.callback.clone()).unwrap();
            }
        }
        // delete timeout entries
        for token_out in time_outs {
            self.timer_list.remove(&token_out);
        }
    }
}
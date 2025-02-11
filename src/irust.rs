mod art;
mod cargo_cmds;
mod events;
mod format;
mod global_variables;
mod help;
pub mod highlight;
mod history;
pub mod options;
mod parser;
mod racer;
mod repl;
mod script;
use crossterm::event::KeyModifiers;
use crossterm::event::{Event, KeyCode, KeyEvent};
use global_variables::GlobalVariables;
use highlight::theme::Theme;
use history::History;
use once_cell::sync::Lazy;
use options::Options;
use printer::{buffer::Buffer, printer::Printer};
use racer::Racer;
use repl::Repl;
use script::ScriptManager;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
static SOUT: Lazy<std::io::Stdout> = Lazy::new(std::io::stdout);

pub struct IRust {
    buffer: Buffer,
    repl: Repl,
    printer: Printer<std::io::StdoutLock<'static>>,
    options: Options,
    racer: Option<Racer>,
    global_variables: GlobalVariables,
    theme: Theme,
    history: History,
    script_mg: Option<ScriptManager>,
}

impl IRust {
    pub fn new(options: Options) -> Self {
        let out = SOUT.lock();
        // Make sure to call Repl::new at the start so it can set `irust-repl` dir, which might be used by others (ScriptManager)
        let repl = Repl::new();

        let global_variables = GlobalVariables::new();

        let script_mg = if options.activate_scripting {
            ScriptManager::new()
        } else {
            None
        };

        let prompt = script_mg
            .as_ref()
            .map(|script_mg| {
                if let Some(prompt) = script_mg.input_prompt(&global_variables) {
                    prompt
                } else {
                    options.input_prompt.clone()
                }
            })
            .unwrap_or_else(|| options.input_prompt.clone());

        let printer = Printer::new(out, prompt);

        let racer = if options.enable_racer {
            Racer::start()
        } else {
            None
        };

        let buffer = Buffer::new();
        let theme = highlight::theme::theme().unwrap_or_default();
        let history = History::new().unwrap_or_default();

        IRust {
            repl,
            printer,
            options,
            racer,
            buffer,
            global_variables,
            theme,
            history,
            script_mg,
        }
    }

    fn prepare(&mut self) -> Result<()> {
        // title is optional
        self.printer.writer.raw.set_title(&format!(
            "IRust: {}",
            self.global_variables.get_cwd().display()
        ))?;
        self.repl.prepare_ground(self.options.toolchain)?;
        self.welcome()?;
        self.printer.print_prompt_if_set()?;

        Ok(())
    }

    /// Wrapper over printer.print_input that highlights rust code using current theme
    pub fn print_input(&mut self) -> Result<()> {
        let theme = &self.theme;
        self.printer
            .print_input(&|buffer| highlight::highlight(buffer, theme), &self.buffer)?;
        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        self.prepare()?;

        loop {
            // flush queued output after each key
            // some events that have an inner input loop like ctrl-r/ ctrl-d require flushing inside their respective handler function
            std::io::Write::flush(&mut self.printer.writer.raw)?;

            match crossterm::event::read() {
                Ok(ev) => {
                    let exit = self.handle_input_event(ev)?;
                    if exit {
                        break Ok(());
                    }
                }
                Err(e) => break Err(format!("failed to read input. error: {}", e).into()),
            }
        }
    }

    fn handle_input_event(&mut self, ev: crossterm::event::Event) -> Result<bool> {
        // handle input event
        match ev {
            Event::Mouse(_) => (),
            Event::Resize(width, height) => {
                self.printer.cursor.update_dimensions(width, height);
                //Hack
                self.handle_ctrl_c()?;
            }
            Event::Key(key_event) => match key_event {
                KeyEvent {
                    code: KeyCode::Char(c),
                    modifiers: KeyModifiers::NONE,
                }
                | KeyEvent {
                    code: KeyCode::Char(c),
                    modifiers: KeyModifiers::SHIFT,
                } => {
                    self.handle_character(c)?;
                }
                KeyEvent {
                    code: KeyCode::Char('e'),
                    modifiers: KeyModifiers::CONTROL,
                } => {
                    self.handle_ctrl_e()?;
                }
                KeyEvent {
                    code: KeyCode::Enter,
                    modifiers: KeyModifiers::ALT,
                } => {
                    self.handle_alt_enter()?;
                }
                KeyEvent {
                    code: KeyCode::Enter,
                    ..
                } => {
                    self.handle_enter(false)?;
                }
                KeyEvent {
                    code: KeyCode::Tab, ..
                } => {
                    self.handle_tab()?;
                }
                KeyEvent {
                    code: KeyCode::BackTab,
                    ..
                } => {
                    self.handle_back_tab()?;
                }
                KeyEvent {
                    code: KeyCode::Left,
                    modifiers: KeyModifiers::NONE,
                } => {
                    self.handle_left()?;
                }
                KeyEvent {
                    code: KeyCode::Right,
                    modifiers: KeyModifiers::NONE,
                } => {
                    self.handle_right()?;
                }
                KeyEvent {
                    code: KeyCode::Up, ..
                } => {
                    self.handle_up()?;
                }
                KeyEvent {
                    code: KeyCode::Down,
                    ..
                } => {
                    self.handle_down()?;
                }
                KeyEvent {
                    code: KeyCode::Backspace,
                    ..
                } => {
                    self.handle_backspace()?;
                }
                KeyEvent {
                    code: KeyCode::Char('c'),
                    modifiers: KeyModifiers::CONTROL,
                } => {
                    self.handle_ctrl_c()?;
                }
                KeyEvent {
                    code: KeyCode::Char('d'),
                    modifiers: KeyModifiers::CONTROL,
                } => {
                    return self.handle_ctrl_d();
                }
                KeyEvent {
                    code: KeyCode::Char('z'),
                    modifiers: KeyModifiers::CONTROL,
                } => {
                    self.handle_ctrl_z()?;
                }
                KeyEvent {
                    code: KeyCode::Char('l'),
                    modifiers: KeyModifiers::CONTROL,
                } => {
                    self.handle_ctrl_l()?;
                }
                KeyEvent {
                    code: KeyCode::Char('r'),
                    modifiers: KeyModifiers::CONTROL,
                } => {
                    self.handle_ctrl_r()?;
                }
                KeyEvent {
                    code: KeyCode::Home,
                    ..
                } => {
                    self.handle_home_key()?;
                }
                KeyEvent {
                    code: KeyCode::End, ..
                } => {
                    self.handle_end_key()?;
                }
                KeyEvent {
                    code: KeyCode::Left,
                    modifiers: KeyModifiers::CONTROL,
                } => {
                    self.handle_ctrl_left()?;
                }
                KeyEvent {
                    code: KeyCode::Right,
                    modifiers: KeyModifiers::CONTROL,
                } => {
                    self.handle_ctrl_right()?;
                }
                KeyEvent {
                    code: KeyCode::Delete,
                    ..
                } => {
                    self.handle_del()?;
                }
                keyevent => {
                    // Handle AltGr on windows
                    if keyevent
                        .modifiers
                        .contains(KeyModifiers::CONTROL | KeyModifiers::ALT)
                    {
                        if let KeyCode::Char(c) = keyevent.code {
                            self.handle_character(c)?;
                        }
                    }
                }
            },
        }
        Ok(false)
    }
}
// Scripts
impl IRust {
    pub fn update_input_prompt(&mut self) {
        if let Some(ref script_mg) = self.script_mg {
            if let Some(prompt) = script_mg.input_prompt(&self.global_variables) {
                self.printer.set_prompt(prompt);
            }
        }
    }
    pub fn get_output_prompt(&mut self) -> String {
        if let Some(ref script_mg) = self.script_mg {
            if let Some(prompt) = script_mg.get_output_prompt(&self.global_variables) {
                return prompt;
            }
        }
        //Default
        self.options.output_prompt.clone()
    }
}

impl Drop for IRust {
    fn drop(&mut self) {
        // ignore errors on drop with let _
        let _ = self.exit();
        if std::thread::panicking() {
            let _ = self.printer.writer.raw.write("IRust panicked, to log the error you can redirect stderror to a file, example irust 2>log");
        }
    }
}

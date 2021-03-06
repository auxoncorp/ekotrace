use crate::manifest_gen::{
    event_metadata::EventMetadata,
    parser::{
        self, event_name_valid, probe_name_valid, remove_double_quotes, tags_or_desc_valid,
        trimmed_string, trimmed_string_w_space, Parser, ParserConfig, Span,
    },
    probe_metadata::ProbeMetadata,
    source_location::SourceLocation,
    type_hint::TypeHint,
};
use crate::warn;
use nom::{
    branch::alt,
    bytes::complete::{is_not, tag, take, take_till1, take_until},
    character::complete::{char, line_ending, multispace0},
    combinator::{iterator, opt, peek, rest},
    error::ParseError,
    sequence::delimited,
};
use nom_locate::position;
use std::fmt;
use std::str::FromStr;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct CParser<'a> {
    pub config: ParserConfig<'a>,
}

impl<'a> Default for CParser<'a> {
    fn default() -> Self {
        CParser {
            config: ParserConfig {
                prefix: "MODALITY_PROBE",
            },
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum Error {
    Syntax(SourceLocation),
    MissingSemicolon(SourceLocation),
    UnrecognizedTypeHint(SourceLocation),
    TypeHintNameNotUpperCase(SourceLocation),
    PayloadArgumentSpansManyLines(SourceLocation),
    EmptyTags(SourceLocation),
    EmptySeverity(SourceLocation),
    SeverityNotNumeric(SourceLocation),
}

impl Error {
    pub fn location(&self) -> &SourceLocation {
        match self {
            Error::Syntax(l) => l,
            Error::MissingSemicolon(l) => l,
            Error::UnrecognizedTypeHint(l) => l,
            Error::TypeHintNameNotUpperCase(l) => l,
            Error::PayloadArgumentSpansManyLines(l) => l,
            Error::EmptyTags(l) => l,
            Error::EmptySeverity(l) => l,
            Error::SeverityNotNumeric(l) => l,
        }
    }
}

impl<'a> Parser for CParser<'a> {
    fn parse_events(&self, input: &str) -> Result<Vec<EventMetadata>, parser::Error> {
        let md = self.parse_event_md(input)?;
        Ok(md)
    }

    fn parse_probes(&self, input: &str) -> Result<Vec<ProbeMetadata>, parser::Error> {
        let md = self.parse_probe_md(input)?;
        Ok(md)
    }
}

impl<'a> CParser<'a> {
    pub fn new(config: ParserConfig<'a>) -> Self {
        CParser { config }
    }

    pub fn parse_event_md(&self, input: &str) -> Result<Vec<EventMetadata>, Error> {
        parse_input(&self.config, input, parse_record_event_call_exp)
    }

    pub fn parse_probe_md(&self, input: &str) -> Result<Vec<ProbeMetadata>, Error> {
        parse_input(&self.config, input, parse_init_call_exp)
    }
}

fn parse_input<T>(
    config: &ParserConfig,
    input: &str,
    parse_fn: fn(Span) -> ParserResult<Span, T>,
) -> Result<Vec<T>, Error> {
    let mut md = vec![];
    let mut input = Span::new_extra(input, Some(config));
    while !input.fragment().is_empty() {
        match parse_fn(input) {
            Ok((rem, metadata)) => {
                md.push(metadata);
                input = rem;
            }
            Err(e) => match e {
                nom::Err::Incomplete(_) => {
                    break;
                }
                nom::Err::Error(int_err) => {
                    let res: nom::IResult<Span, _> = take(1usize)(int_err.into_inner());
                    if let Ok((rem, _)) = res {
                        input = rem;
                    } else {
                        break;
                    }
                }
                nom::Err::Failure(e) => match e.kind {
                    InternalErrorKind::Nom(_, _) => break,
                    InternalErrorKind::Error(_, err) => return Err(err),
                },
            },
        }
    }
    Ok(md)
}

fn parse_record_event_call_exp(input: Span) -> ParserResult<Span, EventMetadata> {
    let prefix = input.extra.as_ref().unwrap().prefix;
    let (input, _) = comments_and_spacing(input)?;
    let expect_w_time_tag = format!("{}_EXPECT_W_TIME", prefix);
    let (input, found_expect_w_time) = peek(opt(tag(expect_w_time_tag.as_str())))(input)?;
    let expect_tag = format!("{}_EXPECT", prefix);
    let (input, found_expect) = peek(opt(tag(expect_tag.as_str())))(input)?;
    let failure_w_time_tag = format!("{}_FAILURE_W_TIME", prefix);
    let (input, found_failure_w_time) = peek(opt(tag(failure_w_time_tag.as_str())))(input)?;
    let failure_tag = format!("{}_FAILURE", prefix);
    let (input, found_failure) = peek(opt(tag(failure_tag.as_str())))(input)?;
    let with_time_tag = format!("{}_RECORD_W_TIME", prefix);
    let (input, found_with_time) = peek(opt(tag(with_time_tag.as_str())))(input)?;
    if found_expect_w_time.is_some() {
        let (input, metadata) = expect_w_time_call_exp(input)?;
        Ok((input, metadata))
    } else if found_expect.is_some() {
        let (input, metadata) = expect_call_exp(input)?;
        Ok((input, metadata))
    } else if found_failure_w_time.is_some() {
        let (input, metadata) = failure_w_time_call_exp(input)?;
        Ok((input, metadata))
    } else if found_failure.is_some() {
        let (input, metadata) = failure_call_exp(input)?;
        Ok((input, metadata))
    } else if found_with_time.is_some() {
        let (input, metadata) = event_with_time(input)?;
        Ok((input, metadata))
    } else {
        let tag_string = format!("{}_RECORD_W_", prefix);
        let (input, found_with_payload) = peek(opt(tag(tag_string.as_str())))(input)?;
        let (input, metadata) = match found_with_payload {
            None => event_call_exp(input)?,
            Some(_) => event_with_payload_call_exp(input)?,
        };
        Ok((input, metadata))
    }
}

fn event_with_time(input: Span) -> ParserResult<Span, EventMetadata> {
    let prefix = input.extra.as_ref().unwrap().prefix;
    let tag_string = format!("{}_RECORD_W_TIME", prefix);
    let (input, pos) = position(input)?;
    let (input, _) = tag(tag_string.as_str())(input)?;
    let (input, _) = opt(line_ending)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, args) = take_until(");")(input)
        .map_err(|e| convert_error(e, Error::MissingSemicolon(pos.into())))?;
    let (input, _) =
        tag(");")(input).map_err(|e| convert_error(e, Error::MissingSemicolon(pos.into())))?;
    let (args, probe_instance) = variable_call_exp_arg(args)?;
    let (args, name) = variable_call_exp_arg(args)?;
    if !event_name_valid(&name) {
        return Err(make_failure(input, Error::Syntax(pos.into())));
    }
    let mut arg_vec: Vec<String> = Vec::new();
    let mut iter = iterator(args, multi_variable_call_exp_arg_literal);
    iter.for_each(|s| arg_vec.push(s));
    let (_args, _) = iter.finish()?;
    match arg_vec.len() {
        1..=3 => (), // At least a payload, maybe tags and description
        _ => return Err(make_failure(input, Error::Syntax(pos.into()))),
    }
    let payload = arg_vec.remove(0).trim().to_string();
    // Check for equal open/close parentheses
    let open = payload.chars().filter(|&c| c == '(').count();
    let close = payload.chars().filter(|&c| c == ')').count();
    if open != close {
        return Err(make_failure(input, Error::Syntax(pos.into())));
    }
    let mut tags_and_desc = arg_vec;
    for s in tags_and_desc.iter_mut() {
        *s = truncate_and_trim(s).map_err(|_| make_failure(input, Error::Syntax(pos.into())))?;
    }
    let tags_pos = tags_and_desc.iter().position(|s| s.contains("tags="));
    let tags = tags_pos
        .map(|index| tags_and_desc.swap_remove(index))
        .map(|s| s.replace("tags=", ""));
    if let Some(t) = &tags {
        if t.is_empty() {
            return Err(make_failure(input, Error::EmptyTags(pos.into())));
        }
    }
    let description = tags_and_desc.pop();
    Ok((
        input,
        EventMetadata {
            name,
            probe_instance,
            payload: None,
            description,
            tags,
            location: pos.into(),
        },
    ))
}

fn expect_call_exp(input: Span) -> ParserResult<Span, EventMetadata> {
    let prefix = input.extra.as_ref().unwrap().prefix;
    let tag_string = format!("{}_EXPECT", prefix);
    let (input, _) = comments_and_spacing(input)?;
    let (input, pos) = position(input)?;
    let (input, _) = tag(tag_string.as_str())(input)?;
    let (input, _) = opt(line_ending)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, args) = take_until(");")(input)
        .map_err(|e| convert_error(e, Error::MissingSemicolon(pos.into())))?;
    let (input, _) =
        tag(");")(input).map_err(|e| convert_error(e, Error::MissingSemicolon(pos.into())))?;
    let (args, probe_instance) = variable_call_exp_arg(args)?;
    let (args, name) = variable_call_exp_arg(args)?;
    if !event_name_valid(&name) {
        return Err(make_failure(input, Error::Syntax(pos.into())));
    }
    let mut arg_vec: Vec<String> = Vec::new();
    let mut iter = iterator(args, multi_variable_call_exp_arg_literal);
    iter.for_each(|s| arg_vec.push(s));
    let (_args, _) = iter.finish()?;
    match arg_vec.len() {
        1..=4 => (), // At least an expression, maybe tags, description, severity
        _ => return Err(make_failure(input, Error::Syntax(pos.into()))),
    }
    let expr = arg_vec.remove(0).trim().to_string();
    let mut tags_and_desc = arg_vec;
    for s in tags_and_desc.iter_mut() {
        if !s.contains("SEVERITY") {
            *s =
                truncate_and_trim(s).map_err(|_| make_failure(input, Error::Syntax(pos.into())))?;
        }
    }
    let tags_pos = tags_and_desc.iter().position(|s| s.contains("tags="));
    let mut tags = tags_pos
        .map(|index| tags_and_desc.swap_remove(index))
        .map(|s| s.replace("tags=", ""));
    let severity_pos = tags_and_desc.iter().position(|s| s.contains("SEVERITY"));
    let severity = severity_pos.map(|index| tags_and_desc.swap_remove(index));
    match (&mut tags, severity) {
        (Some(t), Some(s)) => t.insert_str(0, &format!("{};", s)),
        (None, Some(s)) => tags = Some(s),
        _ => (),
    }
    if let Some(t) = &mut tags {
        if t.is_empty() {
            return Err(make_failure(input, Error::EmptyTags(pos.into())));
        }
        if !t.contains("EXPECTATION") {
            t.insert_str(0, "EXPECTATION;");
        }
        *t = remove_double_quotes(t);
    } else {
        tags = Some(String::from("EXPECTATION"));
    }
    let description = tags_and_desc.pop();
    Ok((
        input,
        EventMetadata {
            name,
            probe_instance,
            payload: Some((TypeHint::U32, expr).into()),
            description,
            tags,
            location: pos.into(),
        },
    ))
}

fn expect_w_time_call_exp(input: Span) -> ParserResult<Span, EventMetadata> {
    let prefix = input.extra.as_ref().unwrap().prefix;
    let tag_string = format!("{}_EXPECT_W_TIME", prefix);
    let (input, _) = comments_and_spacing(input)?;
    let (input, pos) = position(input)?;
    let (input, _) = tag(tag_string.as_str())(input)?;
    let (input, _) = opt(line_ending)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, args) = take_until(");")(input)
        .map_err(|e| convert_error(e, Error::MissingSemicolon(pos.into())))?;
    let (input, _) =
        tag(");")(input).map_err(|e| convert_error(e, Error::MissingSemicolon(pos.into())))?;
    let (args, probe_instance) = variable_call_exp_arg(args)?;
    let (args, name) = variable_call_exp_arg(args)?;
    if !event_name_valid(&name) {
        return Err(make_failure(input, Error::Syntax(pos.into())));
    }
    let mut arg_vec: Vec<String> = Vec::new();
    let mut iter = iterator(args, multi_variable_call_exp_arg_literal);
    iter.for_each(|s| arg_vec.push(s));
    let (_args, _) = iter.finish()?;
    match arg_vec.len() {
        2..=5 => (),
        _ => return Err(make_failure(input, Error::Syntax(pos.into()))),
    }
    let expr = arg_vec.remove(0).trim().to_string();
    let _time = arg_vec.remove(0);
    let mut tags_and_desc = arg_vec;
    for s in tags_and_desc.iter_mut() {
        if !s.contains("SEVERITY") {
            *s =
                truncate_and_trim(s).map_err(|_| make_failure(input, Error::Syntax(pos.into())))?;
        }
    }
    let tags_pos = tags_and_desc.iter().position(|s| s.contains("tags="));
    let mut tags = tags_pos
        .map(|index| tags_and_desc.swap_remove(index))
        .map(|s| s.replace("tags=", ""));
    let severity_pos = tags_and_desc.iter().position(|s| s.contains("SEVERITY"));
    let severity = severity_pos.map(|index| tags_and_desc.swap_remove(index));
    match (&mut tags, severity) {
        (Some(t), Some(s)) => t.insert_str(0, &format!("{};", s)),
        (None, Some(s)) => tags = Some(s),
        _ => (),
    }
    if let Some(t) = &mut tags {
        if t.is_empty() {
            return Err(make_failure(input, Error::EmptyTags(pos.into())));
        }
        if !t.contains("EXPECTATION") {
            t.insert_str(0, "EXPECTATION;");
        }
        *t = remove_double_quotes(t);
    } else {
        tags = Some(String::from("EXPECTATION"));
    }
    let description = tags_and_desc.pop();
    Ok((
        input,
        EventMetadata {
            name,
            probe_instance,
            payload: Some((TypeHint::U32, expr).into()),
            description,
            tags,
            location: pos.into(),
        },
    ))
}

fn failure_call_exp(input: Span) -> ParserResult<Span, EventMetadata> {
    let prefix = input.extra.as_ref().unwrap().prefix;
    let tag_string = format!("{}_FAILURE", prefix);
    let (input, pos) = position(input)?;
    let (input, _) = tag(tag_string.as_str())(input)?;
    let (input, _) = opt(line_ending)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, args) = take_until(");")(input)
        .map_err(|e| convert_error(e, Error::MissingSemicolon(pos.into())))?;
    let (input, _) =
        tag(");")(input).map_err(|e| convert_error(e, Error::MissingSemicolon(pos.into())))?;
    let (args, probe_instance) = variable_call_exp_arg(args)?;
    let expect_tags_or_desc = peek(variable_call_exp_arg)(args).is_ok();
    let (args, name) = if expect_tags_or_desc {
        variable_call_exp_arg(args)?
    } else {
        rest_string(args)?
    };
    if !event_name_valid(&name) {
        return Err(make_failure(input, Error::Syntax(pos.into())));
    }
    let mut tags_and_desc: Vec<String> = Vec::new();
    let mut iter = iterator(args, multi_variable_call_exp_arg_literal);
    iter.for_each(|s| tags_and_desc.push(s));
    let (_args, _) = iter.finish()?;
    if tags_and_desc.len() > 3 {
        return Err(make_failure(input, Error::Syntax(pos.into())));
    }
    for s in tags_and_desc.iter_mut() {
        if !s.contains("SEVERITY") {
            *s =
                truncate_and_trim(s).map_err(|_| make_failure(input, Error::Syntax(pos.into())))?;
        }
    }
    let tags_pos = tags_and_desc.iter().position(|s| s.contains("tags="));
    let mut tags = tags_pos
        .map(|index| tags_and_desc.remove(index))
        .map(|s| s.replace("tags=", ""));
    let severity_pos = tags_and_desc.iter().position(|s| s.contains("SEVERITY"));
    let severity = severity_pos.map(|index| tags_and_desc.swap_remove(index));
    match (&mut tags, severity) {
        (Some(t), Some(s)) => t.insert_str(0, &format!("{};", s)),
        (None, Some(s)) => tags = Some(s),
        _ => (),
    }
    if let Some(t) = &mut tags {
        if t.is_empty() {
            return Err(make_failure(input, Error::EmptyTags(pos.into())));
        }
        if !t.contains("FAILURE") {
            t.insert_str(0, "FAILURE;");
        }
        *t = remove_double_quotes(t);
    } else {
        tags = Some(String::from("FAILURE"));
    }
    let description = tags_and_desc.pop();
    Ok((
        input,
        EventMetadata {
            name,
            probe_instance,
            payload: None,
            description,
            tags,
            location: pos.into(),
        },
    ))
}

fn failure_w_time_call_exp(input: Span) -> ParserResult<Span, EventMetadata> {
    let prefix = input.extra.as_ref().unwrap().prefix;
    let tag_string = format!("{}_FAILURE_W_TIME", prefix);
    let (input, pos) = position(input)?;
    let (input, _) = tag(tag_string.as_str())(input)?;
    let (input, _) = opt(line_ending)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, args) = take_until(");")(input)
        .map_err(|e| convert_error(e, Error::MissingSemicolon(pos.into())))?;
    let (input, _) =
        tag(");")(input).map_err(|e| convert_error(e, Error::MissingSemicolon(pos.into())))?;
    let (args, probe_instance) = variable_call_exp_arg(args)?;
    let expect_tags_or_desc = peek(variable_call_exp_arg)(args).is_ok();
    let (args, name) = if expect_tags_or_desc {
        variable_call_exp_arg(args)?
    } else {
        rest_string(args)?
    };
    if !event_name_valid(&name) {
        return Err(make_failure(input, Error::Syntax(pos.into())));
    }
    let mut tags_and_desc: Vec<String> = Vec::new();
    let mut iter = iterator(args, multi_variable_call_exp_arg_literal);
    iter.for_each(|s| tags_and_desc.push(s));
    let (_args, _) = iter.finish()?;
    if tags_and_desc.len() > 4 {
        return Err(make_failure(input, Error::Syntax(pos.into())));
    }
    let _time = tags_and_desc.remove(0);
    for s in tags_and_desc.iter_mut() {
        if !s.contains("SEVERITY") {
            *s =
                truncate_and_trim(s).map_err(|_| make_failure(input, Error::Syntax(pos.into())))?;
        }
    }
    let tags_pos = tags_and_desc.iter().position(|s| s.contains("tags="));
    let mut tags = tags_pos
        .map(|index| tags_and_desc.remove(index))
        .map(|s| s.replace("tags=", ""));
    let severity_pos = tags_and_desc.iter().position(|s| s.contains("SEVERITY"));
    let severity = severity_pos.map(|index| tags_and_desc.swap_remove(index));
    match (&mut tags, severity) {
        (Some(t), Some(s)) => t.insert_str(0, &format!("{};", s)),
        (None, Some(s)) => tags = Some(s),
        _ => (),
    }
    if let Some(t) = &mut tags {
        if t.is_empty() {
            return Err(make_failure(input, Error::EmptyTags(pos.into())));
        }
        if !t.contains("FAILURE") {
            t.insert_str(0, "FAILURE;");
        }
        *t = remove_double_quotes(t);
    } else {
        tags = Some(String::from("FAILURE"));
    }
    let description = tags_and_desc.pop();
    Ok((
        input,
        EventMetadata {
            name,
            probe_instance,
            payload: None,
            description,
            tags,
            location: pos.into(),
        },
    ))
}

fn event_call_exp(input: Span) -> ParserResult<Span, EventMetadata> {
    let prefix = input.extra.as_ref().unwrap().prefix;
    let tag_string = format!("{}_RECORD", prefix);
    let (input, pos) = position(input)?;
    let (input, _) = tag(tag_string.as_str())(input)?;
    let (input, _) = opt(line_ending)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, args) = take_until(");")(input)
        .map_err(|e| convert_error(e, Error::MissingSemicolon(pos.into())))?;
    let (input, _) =
        tag(");")(input).map_err(|e| convert_error(e, Error::MissingSemicolon(pos.into())))?;
    let (args, probe_instance) = variable_call_exp_arg(args)?;
    let expect_tags_or_desc = peek(variable_call_exp_arg)(args).is_ok();
    let (args, name) = if expect_tags_or_desc {
        variable_call_exp_arg(args)?
    } else {
        rest_string(args)?
    };
    if !event_name_valid(&name) {
        return Err(make_failure(input, Error::Syntax(pos.into())));
    }
    let mut tags_and_desc: Vec<String> = Vec::new();
    let mut iter = iterator(args, multi_variable_call_exp_arg_literal);
    iter.for_each(|s| tags_and_desc.push(s));
    let (_args, _) = iter.finish()?;
    if tags_and_desc.len() > 2 {
        return Err(make_failure(input, Error::Syntax(pos.into())));
    }
    for s in tags_and_desc.iter_mut() {
        *s = truncate_and_trim(s).map_err(|_| make_failure(input, Error::Syntax(pos.into())))?;
    }
    let tags_pos = tags_and_desc.iter().position(|s| s.contains("tags="));
    let tags = tags_pos
        .map(|index| tags_and_desc.remove(index))
        .map(|s| s.replace("tags=", ""));
    if let Some(t) = &tags {
        if t.is_empty() {
            return Err(make_failure(input, Error::EmptyTags(pos.into())));
        }
    }
    let description = tags_and_desc.pop();
    Ok((
        input,
        EventMetadata {
            name,
            probe_instance,
            payload: None,
            description,
            tags,
            location: pos.into(),
        },
    ))
}

fn event_with_payload_call_exp(input: Span) -> ParserResult<Span, EventMetadata> {
    let prefix = input.extra.as_ref().unwrap().prefix;
    let tag_string = format!("{}_RECORD_W_", prefix);
    let (input, pos) = position(input)?;
    let (input, _) = tag(tag_string.as_str())(input)?;
    let (input, type_hint) = take_until("(")(input)?;
    let mut type_hint = type_hint.to_string();
    let has_time = type_hint.contains("_W_TIME");
    if has_time {
        type_hint = type_hint.replace("_W_TIME", "");
    }
    if type_hint.to_uppercase().as_str() != type_hint.as_str() {
        return Err(make_failure(
            input,
            Error::TypeHintNameNotUpperCase(pos.into()),
        ));
    }
    let type_hint = TypeHint::from_str(type_hint.as_str())
        .map_err(|_| make_failure(input, Error::UnrecognizedTypeHint(pos.into())))?;
    let (input, _) = opt(line_ending)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, args) = take_until(");")(input)
        .map_err(|e| convert_error(e, Error::MissingSemicolon(pos.into())))?;
    let (input, _) =
        tag(");")(input).map_err(|e| convert_error(e, Error::MissingSemicolon(pos.into())))?;
    let (args, probe_instance) = variable_call_exp_arg(args)?;
    let (args, name) = variable_call_exp_arg(args)?;
    if !event_name_valid(&name) {
        return Err(make_failure(input, Error::Syntax(pos.into())));
    }
    let mut arg_vec: Vec<String> = Vec::new();
    let mut iter = iterator(args, multi_variable_call_exp_arg_literal);
    iter.for_each(|s| arg_vec.push(s));
    let (_args, _) = iter.finish()?;
    if has_time {
        match arg_vec.len() {
            2..=4 => (), // At least payload and time, maybe tags and description
            _ => return Err(make_failure(input, Error::Syntax(pos.into()))),
        }
    } else {
        match arg_vec.len() {
            1..=3 => (), // At least a payload, maybe tags and description
            _ => return Err(make_failure(input, Error::Syntax(pos.into()))),
        }
    }
    // We have a constraint that the payload argument doesn't span
    // multiple lines, trim off leading and trailing space
    let payload = arg_vec.remove(0).trim().to_string();
    for c in payload.chars() {
        if c == '\n' {
            return Err(make_failure(
                input,
                Error::PayloadArgumentSpansManyLines(pos.into()),
            ));
        }
    }
    // Check for equal open/close parentheses
    let open = payload.chars().filter(|&c| c == '(').count();
    let close = payload.chars().filter(|&c| c == ')').count();
    if open != close {
        return Err(make_failure(input, Error::Syntax(pos.into())));
    }
    if has_time {
        let _time = arg_vec.remove(0);
    }
    let mut tags_and_desc = arg_vec;
    for s in tags_and_desc.iter_mut() {
        *s = truncate_and_trim(s).map_err(|_| make_failure(input, Error::Syntax(pos.into())))?;
    }
    let tags_pos = tags_and_desc.iter().position(|s| s.contains("tags="));
    let tags = tags_pos
        .map(|index| tags_and_desc.swap_remove(index))
        .map(|s| s.replace("tags=", ""));

    if let Some(t) = &tags {
        if t.is_empty() {
            return Err(make_failure(input, Error::EmptyTags(pos.into())));
        }
    }
    let description = tags_and_desc.pop();
    Ok((
        input,
        EventMetadata {
            name,
            probe_instance,
            payload: Some((type_hint, payload).into()),
            description,
            tags,
            location: pos.into(),
        },
    ))
}

fn variable_call_exp_arg(input: Span) -> ParserResult<Span, String> {
    let (input, _) = comments_and_spacing(input)?;
    let (input, arg) = take_until(",")(input)?;
    let (input, _) = tag(",")(input)?;
    Ok((input, trimmed_string(arg.fragment())))
}

fn multi_variable_call_exp_arg_literal(input: Span) -> ParserResult<Span, String> {
    let (input, _) = comments_and_spacing(input)?;
    if input.fragment().is_empty() {
        return Err(nom::Err::Error(
            (input, nom::error::ErrorKind::ParseTo).into(),
        ));
    }
    let (input, expect_tags) = peek(opt(tag("MODALITY_TAGS")))(input)?;
    let (input, expect_severity) = peek(opt(tag("MODALITY_SEVERITY")))(input)?;
    let expect_another = peek(variable_call_exp_arg_literal)(input).is_ok();
    let (input, arg) = if expect_tags.is_some() {
        modality_tags(input)?
    } else if expect_severity.is_some() {
        modality_severity_as_tag(input)?
    } else if expect_another {
        variable_call_exp_arg_literal(input)?
    } else {
        rest_literal(input)?
    };
    Ok((input, arg))
}

fn variable_call_exp_arg_literal(input: Span) -> ParserResult<Span, String> {
    let (input, _) = comments_and_spacing(input)?;
    let (input, arg) = take_until(",")(input)?;
    let (input, _) = tag(",")(input)?;
    Ok((input, (*arg.fragment()).to_string()))
}

fn parse_init_call_exp(input: Span) -> ParserResult<Span, ProbeMetadata> {
    let prefix = input.extra.as_ref().unwrap().prefix;
    let tag_string = format!("{}_INIT", prefix);
    let (input, _) = comments_and_spacing(input)?;
    let (input, pos) = position(input)?;
    let (input, _) = tag(tag_string.as_str())(input)?;
    let (input, _) = opt(line_ending)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("(")(input)?;
    let (input, args) = take_until(");")(input)
        .map_err(|e| convert_error(e, Error::MissingSemicolon(pos.into())))?;
    let (input, _) =
        tag(");")(input).map_err(|e| convert_error(e, Error::MissingSemicolon(pos.into())))?;
    let (args, _storage) =
        variable_call_exp_arg(args).map_err(|e| convert_error(e, Error::Syntax(pos.into())))?;
    let (args, _storage_size) =
        variable_call_exp_arg(args).map_err(|e| convert_error(e, Error::Syntax(pos.into())))?;
    let (args, name) =
        variable_call_exp_arg(args).map_err(|e| convert_error(e, Error::Syntax(pos.into())))?;
    if !probe_name_valid(&name) {
        return Err(make_failure(input, Error::Syntax(pos.into())));
    }
    let (args, _time_res) =
        variable_call_exp_arg(args).map_err(|e| convert_error(e, Error::Syntax(pos.into())))?;
    let (args, _wall_clock_id) =
        variable_call_exp_arg(args).map_err(|e| convert_error(e, Error::Syntax(pos.into())))?;
    let (args, _next_seq_id_fn) =
        variable_call_exp_arg(args).map_err(|e| convert_error(e, Error::Syntax(pos.into())))?;
    let (args, _next_seq_id_state) =
        variable_call_exp_arg(args).map_err(|e| convert_error(e, Error::Syntax(pos.into())))?;
    let expect_tags_or_desc = peek(variable_call_exp_arg)(args).is_ok();
    let (args, _probe_instance) = if expect_tags_or_desc {
        variable_call_exp_arg(args).map_err(|e| convert_error(e, Error::Syntax(pos.into())))?
    } else {
        rest_string(args).map_err(|e| convert_error(e, Error::Syntax(pos.into())))?
    };
    let mut tags_and_desc: Vec<String> = Vec::new();
    let mut iter = iterator(args, multi_variable_call_exp_arg_literal);
    iter.for_each(|s| tags_and_desc.push(s));
    let (_args, _) = iter.finish()?;
    if tags_and_desc.len() > 2 {
        return Err(make_failure(input, Error::Syntax(pos.into())));
    }
    for s in tags_and_desc.iter_mut() {
        *s = truncate_and_trim(s).map_err(|_| make_failure(input, Error::Syntax(pos.into())))?;
    }
    let tags_pos = tags_and_desc.iter().position(|s| s.contains("tags="));
    let tags = tags_pos
        .map(|index| tags_and_desc.remove(index))
        .map(|s| s.replace("tags=", ""));
    if let Some(t) = &tags {
        if t.is_empty() {
            return Err(make_failure(input, Error::EmptyTags(pos.into())));
        }
    }
    let description = tags_and_desc.pop();
    Ok((
        input,
        ProbeMetadata {
            name,
            location: pos.into(),
            tags,
            description,
        },
    ))
}

fn modality_tags(input: Span) -> ParserResult<Span, String> {
    let (input, _) = comments_and_spacing(input)?;
    let (input, pos) = position(input)?;
    let (input, _) = tag("MODALITY_TAGS")(input)?;
    let (input, args) = delimited(char('('), is_not(")"), char(')'))(input)
        .map_err(|e| convert_error(e, Error::EmptyTags(pos.into())))?;
    let (input, _) = opt(tag(","))(input)?;
    let split: Vec<&str> = args.fragment().split(',').collect();
    if split.is_empty() {
        return Err(make_failure(input, Error::Syntax(pos.into())));
    }
    let mut tags = String::from("tags=");
    for (idx, s) in split.iter().enumerate() {
        let s = if !s.contains('"') {
            format!("\"{}\"", s)
        } else {
            s.to_string()
        };
        let t =
            truncate_and_trim(&s).map_err(|_| make_failure(input, Error::Syntax(pos.into())))?;
        tags.push_str(&t);
        if (split.len() > 1) && (idx < (split.len() - 1)) {
            tags.push(';');
        }
    }
    let tags = format!("\"{}\"", tags);
    Ok((input, tags))
}

fn modality_severity_as_tag(input: Span) -> ParserResult<Span, String> {
    let (input, _) = comments_and_spacing(input)?;
    let (input, pos) = position(input)?;
    let (input, _) = tag("MODALITY_SEVERITY")(input)?;
    let (input, level) = delimited(char('('), is_not(")"), char(')'))(input)
        .map_err(|e| convert_error(e, Error::EmptySeverity(pos.into())))?;
    let (input, _) = opt(tag(","))(input)?;
    let level_num = level
        .fragment()
        .parse::<u8>()
        .map_err(|_| make_failure(input, Error::SeverityNotNumeric(pos.into())))?;
    let clamped_level_num = if level_num < 1 {
        warn!(
            "manifest-gen",
            "Clamping invalid severity level {} to 1", level_num
        );
        1
    } else if level_num > 10 {
        warn!(
            "manifest-gen",
            "Clamping invalid severity level {} to 10", level_num
        );
        10
    } else {
        level_num
    };
    let severity_tag = format!("SEVERITY_{}", clamped_level_num);
    Ok((input, severity_tag))
}

fn comments_and_spacing(input: Span) -> ParserResult<Span, ()> {
    let (input, _) = opt(line_ending)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = comment(input)?;
    let (input, _) = opt(line_ending)(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = comment(input)?;
    Ok((input, ()))
}

fn comment(input: Span) -> ParserResult<Span, ()> {
    let (input, maybe_comment) = opt(alt((tag("///"), tag("//"))))(input)?;
    let input = if maybe_comment.is_some() {
        let (input, _) = take_till1(|c| c == '\n')(input)?;
        input
    } else {
        input
    };

    let (input, maybe_comment) = opt(tag("/*"))(input)?;
    let input = if maybe_comment.is_some() {
        let (input, _) = take_until("*/")(input)?;
        let (input, _) = tag("*/")(input)?;
        input
    } else {
        input
    };

    Ok((input, ()))
}

fn rest_string(input: Span) -> ParserResult<Span, String> {
    let (input, _) = comments_and_spacing(input)?;
    let (input, rst) = rest(input)?;
    Ok((input, trimmed_string(rst.fragment())))
}

fn rest_literal(input: Span) -> ParserResult<Span, String> {
    let (input, _) = comments_and_spacing(input)?;
    let (input, rst) = rest(input)?;
    Ok((input, (*rst.fragment()).to_string()))
}

fn truncate_and_trim(s: &str) -> Result<String, ()> {
    let arg = Span::new_extra(s, None);
    let (arg, _) = comments_and_spacing(arg).map_err(|_| ())?;
    let tail_index = arg.fragment().rfind('"').ok_or(())?;
    if tail_index == 0 {
        return Err(());
    }
    let mut s = (*arg.fragment()).to_string();
    s.truncate(tail_index + 1);
    s = trimmed_string_w_space(&s);
    if !tags_or_desc_valid(&s) {
        return Err(());
    }
    s = remove_double_quotes(&s);
    Ok(s)
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::MissingSemicolon(_) => write!(
                f,
                "Record event call-site is missing a semicolon",
            ),
            Error::UnrecognizedTypeHint(_) => write!(
                f,
                "Record event with payload call-site has an unrecognized payload type hint",
            ),
            Error::TypeHintNameNotUpperCase(_) => write!(
                f,
                "Record event with payload call-site has a payload type hint that needs to be upper case",
            ),
            Error::PayloadArgumentSpansManyLines(_) => write!(
                f,
                "Record event with payload call-site has a payload argument that spans many lines",
            ),
            Error::Syntax(_) => write!(
                f,
                "Enountered a syntax error while parsing a record event call-site",
            ),
            Error::EmptyTags(_) => write!(
                f,
                "Enountered an empty tags statement while parsing a record event call-site",
            ),
            Error::EmptySeverity(_) => write!(
                f,
                "Enountered an empty severity level statement while parsing a record event call-site",
            ),
            Error::SeverityNotNumeric(_) => write!(
                f,
                "Enountered an invalid non-numeric severity level statement while parsing a record event call-site",
            ),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
enum InternalErrorKind<I> {
    Nom(I, nom::error::ErrorKind),
    Error(I, Error),
}

type ParserResult<I, O> = nom::IResult<I, O, InternalError<I>>;

impl<I> ParseError<I> for InternalError<I> {
    fn from_error_kind(input: I, kind: nom::error::ErrorKind) -> Self {
        Self {
            kind: InternalErrorKind::Nom(input, kind),
            backtrace: Vec::new(),
        }
    }

    fn append(input: I, kind: nom::error::ErrorKind, mut other: Self) -> Self {
        other.backtrace.push(InternalErrorKind::Nom(input, kind));
        other
    }
}

fn convert_error<I>(nom_err: nom::Err<InternalError<I>>, err: Error) -> nom::Err<InternalError<I>> {
    match nom_err {
        nom::Err::Failure(i) | nom::Err::Error(i) => {
            nom::Err::Failure((i.into_inner(), err).into())
        }
        nom::Err::Incomplete(i) => nom::Err::Incomplete(i),
    }
}

fn make_failure<I>(input: I, err: Error) -> nom::Err<InternalError<I>> {
    nom::Err::Failure((input, err).into())
}

impl<I> From<(I, nom::error::ErrorKind)> for InternalErrorKind<I> {
    fn from(e: (I, nom::error::ErrorKind)) -> Self {
        InternalErrorKind::Nom(e.0, e.1)
    }
}

impl<I> From<(I, Error)> for InternalErrorKind<I> {
    fn from(e: (I, Error)) -> Self {
        InternalErrorKind::Error(e.0, e.1)
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
struct InternalError<I> {
    kind: InternalErrorKind<I>,
    backtrace: Vec<InternalErrorKind<I>>,
}

impl<I> InternalError<I> {
    fn into_inner(self) -> I {
        match self.kind {
            InternalErrorKind::Nom(i, _) => i,
            InternalErrorKind::Error(i, _) => i,
        }
    }
}

impl<I> From<(I, nom::error::ErrorKind)> for InternalError<I> {
    fn from(e: (I, nom::error::ErrorKind)) -> Self {
        Self {
            kind: (e.0, e.1).into(),
            backtrace: Vec::new(),
        }
    }
}

impl<I> From<(I, Error)> for InternalError<I> {
    fn from(e: (I, Error)) -> Self {
        Self {
            kind: (e.0, e.1).into(),
            backtrace: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const MIXED_PROBE_ID_INPUT: &'static str = r#"
    /* C/C++ style */
    modality_probe_error result = MODALITY_PROBE_INIT(
        destination,
        DEFAULT_PROBE_SIZE,
        DEFAULT_PROBE_ID,
        0,
        0,
        NULL,
        NULL,
        &t);

    // One line
    MODALITY_PROBE_INIT(dest,PROBE_SIZE,MY_PROBE_ID,0, 0,&my_next_seq_id_fn,&my_state,&t);

    const size_t err = MODALITY_PROBE_INIT(     dest,  PROBE_SIZE,
    PROBE_ID_FOO,      TIME_RES,  CLK_ID,  NULL    ,    NULL, &t);

    const size_t err =
        MODALITY_PROBE_INIT(
        // stuff
        dest, // more stuff
        PROBE_SIZE, /* comment */
    PROBE_ID_BAR,   /* things */   0, 0, NULL, /*comments*/   NULL, &t);

    MODALITY_PROBE_INIT(
        dest, /* more docs */ PROBE_SIZE , /* docs */ MY_OTHER_PROBE_ID, 0, 0, NULL, NULL, /* docs */ &t, "desc");

    /* things in comments
     * are
     * ignored
     *
     * MODALITY_PROBE_INIT(dest,PROBE_SIZE,ANOTHER_ID,0,0,NULL,NULL,&t);
     *
     */
    size_t err = MODALITY_PROBE_INIT(
            &g_agent_storage[0],
            STORAGE_SIZE,
            PROBE_ID_FOO,
            TIME_RES,
            CLK_ID,
            &next_seq_id,
            &next_seq_id_state,
            &g_agent,
            MODALITY_TAGS(my-tags, more tags),
            "Description");
    assert(err == MODALITY_PROBE_ERROR_OK);

    MODALITY_PROBE_INIT(storage, size, ID_BAR, 0, 0, NULL, NULL, t, MODALITY_TAGS(my tag));
"#;

    const MIXED_EVENT_RECORDING_INPUT: &'static str = r#"
    /* The user writes this line: */
    const size_t err = MODALITY_PROBE_RECORD(g_probe, EVENT_READ1);

    assert(err == MODALITY_PROBE_ERROR_OK);

    /*
     * Comments */
    const size_t err = MODALITY_PROBE_RECORD(g_probe, EVENT_READ2, "my docs");

    assert(err == MODALITY_PROBE_ERROR_OK);

    MODALITY_PROBE_RECORD(
            probe, /* comments */
            EVENT_WRITE1,
            MODALITY_TAGS(network)); // more comments

    MODALITY_PROBE_RECORD(  probe, /* comments */ EVENT_WRITE2, MODALITY_TAGS(network, file-system), "docs"); // more comments

    uint8_t status;
    const size_t err = MODALITY_PROBE_RECORD_W_U8(probe, EVENT_A, status);

    const size_t err = MODALITY_PROBE_RECORD_W_U8(
        probe, // stuff
        EVENT_B, /* here */
        status,
        "desc text here"); // The end

    /* stuff
     * MODALITY_PROBE_RECORD_W_U8(probe, SOME_EVENT, status);
     */
    const size_t err = MODALITY_PROBE_RECORD_W_I16(probe, EVENT_C, (int16_t) data);

    const size_t err = MODALITY_PROBE_RECORD_W_I16(probe, EVENT_D, (int16_t) data, "docs");

    const size_t err = MODALITY_PROBE_RECORD_W_I8(probe, EVENT_E,
    (int8_t) *((uint8_t*) &mydata));

    const size_t err = MODALITY_PROBE_RECORD_W_U16(
        probe,
        EVENT_F,
    (uint16_t) *((uint16_t*) &mydata),
    MODALITY_TAGS(my tag)
    );

    const size_t err = MODALITY_PROBE_RECORD_W_U16(
        probe,
        EVENT_G,
    (uint16_t) *((uint16_t*) &mydata),
    " docs ", /* Order of tags and docs doesn't matter */
    MODALITY_TAGS(thing1, thing2, my::namespace, "tag with spaces") // docs
    );

    err = MODALITY_PROBE_EXPECT(
            probe,
            EVENT_H,
            1 == 0, /* Arbitrary expression, evaluates to 0 (failure) or 1 (success) */
            MODALITY_TAGS(another tag),
            MODALITY_SEVERITY(1),
            "Some description");
    assert(err == MODALITY_PROBE_ERROR_OK);

    MODALITY_PROBE_EXPECT(probe, EVENT_I, *foo != (1 + bar), MODALITY_SEVERITY(2),
        MODALITY_TAGS(EXPECTATION, network));

    /* Special "EXPECTATION" tag is inserted"
    MODALITY_PROBE_EXPECT(probe, EVENT_J, 0 == 0);

    err = MODALITY_PROBE_RECORD_W_TIME(
            g_probe,
            EVENT_K,
            1,
            MODALITY_TAGS(network, file-system, "other-tags"),
            "Description");

    MODALITY_PROBE_RECORD_W_I8_W_TIME(
            g_probe,
            EVENT_L,
            status,
            2,
            MODALITY_TAGS(network, file-system, "other-tags"),
            "Description");

    MODALITY_PROBE_RECORD_W_BOOL_W_TIME(g_probe, EVENT_M, true, 1);

    err = MODALITY_PROBE_FAILURE(probe, EVENT_N);
    err = MODALITY_PROBE_FAILURE(probe, EVENT_O,
        "desc",
        MODALITY_TAGS("tag-a"));
    err = MODALITY_PROBE_FAILURE(
        probe,
        EVENT_P,
        MODALITY_SEVERITY(5),
        MODALITY_TAGS("tag-a"),
        "desc");

    err = MODALITY_PROBE_EXPECT_W_TIME(
        probe,
        EVENT_P,
        1 == 0,
        1,
        "desc",
        MODALITY_TAGS("tag-a"),
        MODALITY_SEVERITY(10));
    err = MODALITY_PROBE_FAILURE_W_TIME(
        probe,
        EVENT_P,
        1,
        "desc",
        MODALITY_TAGS("tag-a"),
        MODALITY_SEVERITY(10));
"#;

    #[test]
    fn probe_metadata_in_mixed_input() {
        let parser = CParser::default();
        let tokens = parser.parse_probes(MIXED_PROBE_ID_INPUT);
        assert_eq!(
            tokens,
            Ok(vec![
                ProbeMetadata {
                    name: "DEFAULT_PROBE_ID".to_string(),
                    location: (57, 3, 35).into(),
                    tags: None,
                    description: None,
                },
                ProbeMetadata {
                    name: "MY_PROBE_ID".to_string(),
                    location: (237, 14, 5).into(),
                    tags: None,
                    description: None,
                },
                ProbeMetadata {
                    name: "PROBE_ID_FOO".to_string(),
                    location: (348, 16, 24).into(),
                    tags: None,
                    description: None,
                },
                ProbeMetadata {
                    name: "PROBE_ID_BAR".to_string(),
                    location: (491, 20, 9).into(),
                    tags: None,
                    description: None,
                },
                ProbeMetadata {
                    name: "MY_OTHER_PROBE_ID".to_string(),
                    location: (669, 26, 5).into(),
                    tags: None,
                    description: Some("desc".to_string()),
                },
                ProbeMetadata {
                    name: "PROBE_ID_FOO".to_string(),
                    location: (970, 36, 18).into(),
                    tags: Some("my-tags;more tags".to_string()),
                    description: Some("Description".to_string()),
                },
                ProbeMetadata {
                    name: "ID_BAR".to_string(),
                    location: (1322, 49, 5).into(),
                    tags: Some("my tag".to_string()),
                    description: None,
                },
            ])
        );
    }

    #[test]
    fn event_metadata_in_mixed_input() {
        let parser = CParser::default();
        let tokens = parser.parse_event_md(MIXED_EVENT_RECORDING_INPUT);
        assert_eq!(
            tokens,
            Ok(vec![
                EventMetadata {
                    name: "EVENT_READ1".to_string(),
                    probe_instance: "g_probe".to_string(),
                    payload: None,
                    description: None,
                    tags: None,
                    location: (61, 3, 24).into(),
                },
                EventMetadata {
                    name: "EVENT_READ2".to_string(),
                    probe_instance: "g_probe".to_string(),
                    payload: None,
                    description: Some("my docs".to_string()),
                    tags: None,
                    location: (201, 9, 24).into(),
                },
                EventMetadata {
                    name: "EVENT_WRITE1".to_string(),
                    probe_instance: "probe".to_string(),
                    payload: None,
                    description: None,
                    tags: Some("network".to_string()),
                    location: (307, 13, 5).into(),
                },
                EventMetadata {
                    name: "EVENT_WRITE2".to_string(),
                    probe_instance: "probe".to_string(),
                    payload: None,
                    description: Some("docs".to_string()),
                    tags: Some("network;file-system".to_string()),
                    location: (449, 18, 5).into(),
                },
                EventMetadata {
                    name: "EVENT_A".to_string(),
                    probe_instance: "probe".to_string(),
                    payload: Some((TypeHint::U8, "status").into()),
                    description: None,
                    tags: None,
                    location: (616, 21, 24).into(),
                },
                EventMetadata {
                    name: "EVENT_B".to_string(),
                    probe_instance: "probe".to_string(),
                    payload: Some((TypeHint::U8, "status").into()),
                    description: Some("desc text here".to_string()),
                    tags: None,
                    location: (692, 23, 24).into(),
                },
                EventMetadata {
                    name: "EVENT_C".to_string(),
                    probe_instance: "probe".to_string(),
                    payload: Some((TypeHint::I16, "(int16_t) data").into()),
                    description: None,
                    tags: None,
                    location: (933, 32, 24).into(),
                },
                EventMetadata {
                    name: "EVENT_D".to_string(),
                    probe_instance: "probe".to_string(),
                    payload: Some((TypeHint::I16, "(int16_t) data").into()),
                    description: Some("docs".to_string()),
                    tags: None,
                    location: (1018, 34, 24).into(),
                },
                EventMetadata {
                    name: "EVENT_E".to_string(),
                    probe_instance: "probe".to_string(),
                    payload: Some((TypeHint::I8, "(int8_t) *((uint8_t*) &mydata)").into()),
                    description: None,
                    tags: None,
                    location: (1111, 36, 24).into(),
                },
                EventMetadata {
                    name: "EVENT_F".to_string(),
                    probe_instance: "probe".to_string(),
                    payload: Some((TypeHint::U16, "(uint16_t) *((uint16_t*) &mydata)").into()),
                    description: None,
                    tags: Some("my tag".to_string()),
                    location: (1215, 39, 24).into(),
                },
                EventMetadata {
                    name: "EVENT_G".to_string(),
                    probe_instance: "probe".to_string(),
                    payload: Some((TypeHint::U16, "(uint16_t) *((uint16_t*) &mydata)").into()),
                    description: Some("docs".to_string()),
                    tags: Some("thing1;thing2;my::namespace;tag with spaces".to_string()),
                    location: (1372, 46, 24).into(),
                },
                EventMetadata {
                    name: "EVENT_H".to_string(),
                    probe_instance: "probe".to_string(),
                    payload: Some((TypeHint::U32, "1 == 0").into()),
                    description: Some("Some description".to_string()),
                    tags: Some("EXPECTATION;SEVERITY_1;another tag".to_string()),
                    location: (1624, 54, 11).into(),
                },
                EventMetadata {
                    name: "EVENT_I".to_string(),
                    probe_instance: "probe".to_string(),
                    payload: Some((TypeHint::U32, "*foo != (1 + bar)").into()),
                    description: None,
                    tags: Some("SEVERITY_2;EXPECTATION;network".to_string()),
                    location: (1931, 63, 5).into(),
                },
                EventMetadata {
                    name: "EVENT_J".to_string(),
                    probe_instance: "probe".to_string(),
                    payload: Some((TypeHint::U32, "0 == 0").into()),
                    description: None,
                    tags: Some("EXPECTATION".to_string()),
                    location: (2107, 67, 5).into(),
                },
                EventMetadata {
                    name: "EVENT_K".to_string(),
                    probe_instance: "g_probe".to_string(),
                    payload: None,
                    description: Some("Description".to_string()),
                    tags: Some("network;file-system;other-tags".to_string()),
                    location: (2165, 69, 11).into(),
                },
                EventMetadata {
                    name: "EVENT_L".to_string(),
                    probe_instance: "g_probe".to_string(),
                    payload: Some((TypeHint::I8, "status").into()),
                    description: Some("Description".to_string()),
                    tags: Some("network;file-system;other-tags".to_string()),
                    location: (2348, 76, 5).into(),
                },
                EventMetadata {
                    name: "EVENT_M".to_string(),
                    probe_instance: "g_probe".to_string(),
                    payload: Some((TypeHint::Bool, "true").into()),
                    description: None,
                    tags: None,
                    location: (2556, 84, 5).into(),
                },
                EventMetadata {
                    name: "EVENT_N".to_string(),
                    probe_instance: "probe".to_string(),
                    payload: None,
                    description: None,
                    tags: Some("FAILURE".to_string()),
                    location: (2631, 86, 11).into(),
                },
                EventMetadata {
                    name: "EVENT_O".to_string(),
                    probe_instance: "probe".to_string(),
                    payload: None,
                    description: Some("desc".to_string()),
                    tags: Some("FAILURE;tag-a".to_string()),
                    location: (2681, 87, 11).into(),
                },
                EventMetadata {
                    name: "EVENT_P".to_string(),
                    probe_instance: "probe".to_string(),
                    payload: None,
                    description: Some("desc".to_string()),
                    tags: Some("FAILURE;SEVERITY_5;tag-a".to_string()),
                    location: (2779, 90, 11).into(),
                },
                EventMetadata {
                    name: "EVENT_P".to_string(),
                    probe_instance: "probe".to_string(),
                    payload: Some((TypeHint::U32, "1 == 0").into()),
                    description: Some("desc".to_string()),
                    tags: Some("EXPECTATION;SEVERITY_10;tag-a".to_string()),
                    location: (2925, 97, 11).into(),
                },
                EventMetadata {
                    name: "EVENT_P".to_string(),
                    probe_instance: "probe".to_string(),
                    payload: None,
                    description: Some("desc".to_string()),
                    tags: Some("FAILURE;SEVERITY_10;tag-a".to_string()),
                    location: (3104, 105, 11).into(),
                },
            ])
        );
    }

    #[test]
    fn missing_semicolon_errors() {
        let parser = CParser::default();
        let input = r#"
const size_t err = MODALITY_PROBE_RECORD(g_probe, EVENT_READ)
"#;
        let tokens = parser.parse_event_md(input);
        assert_eq!(tokens, Err(Error::MissingSemicolon((20, 2, 20).into())));
        let input = "const size_t err = MODALITY_PROBE_RECORD(g_probe, EVENT_READ)";
        let tokens = parser.parse_event_md(input);
        assert_eq!(tokens, Err(Error::MissingSemicolon((19, 1, 20).into())));
        let input = "MODALITY_PROBE_RECORD_W_I16(probe, E0, data)";
        let tokens = parser.parse_event_md(input);
        assert_eq!(tokens, Err(Error::MissingSemicolon((0, 1, 1).into())));
        let input = "MODALITY_PROBE_INIT(storage, size, ID_BAR, NULL, NULL, t)";
        let tokens = parser.parse_probe_md(input);
        assert_eq!(tokens, Err(Error::MissingSemicolon((0, 1, 1).into())));
    }

    #[test]
    fn syntax_errors() {
        let parser = CParser::default();
        let input = r#"
const size_t err = MODALITY_PROBE_RECORD_W_U8(g_probe, EVENT_READ, (uint8_t) (( ))))status);
"#;
        let tokens = parser.parse_event_md(input);
        assert_eq!(tokens, Err(Error::Syntax((20, 2, 20).into())));
        let input = r#"
const size_t err = MODALITY_PROBE_RECORD_W_U8(g_probe, EVENT_READ, (uint8_t) status)
assert(err == MODALITY_PROBE_ERROR_OK);
"#;
        let tokens = parser.parse_event_md(input);
        assert_eq!(
            tokens,
            Err(Error::PayloadArgumentSpansManyLines((20, 2, 20).into()))
        );
        let input = r#"
err = MODALITY_PROBE_RECORD_W_U8(
        g_probe,
        EVENT_READ_STATUS2,
        (uint8_t) status,
assert(err == MODALITY_PROBE_ERROR_OK);
"#;
        let tokens = parser.parse_event_md(input);
        assert_eq!(tokens, Err(Error::Syntax((7, 2, 7).into())));
        let input = r#"
err = MODALITY_PROBE_RECORD(
        g_probe,
        EVENT_READ_STATUS2,
assert(err == MODALITY_PROBE_ERROR_OK);
"#;
        let tokens = parser.parse_event_md(input);
        assert_eq!(tokens, Err(Error::Syntax((7, 2, 7).into())));
    }

    #[test]
    fn event_payload_type_hint_errors() {
        let parser = CParser::default();
        let input = "MODALITY_PROBE_RECORD_W_I12(probe, E0, data);";
        let tokens = parser.parse_event_md(input);
        assert_eq!(tokens, Err(Error::UnrecognizedTypeHint((0, 1, 1).into())));
    }

    #[test]
    fn event_payload_casing_errors() {
        let parser = CParser::default();
        let input = "MODALITY_PROBE_RECORD_W_i8(probe, EVENT_A, status);";
        let tokens = parser.parse_event_md(input);
        assert_eq!(
            tokens,
            Err(Error::TypeHintNameNotUpperCase((0, 1, 1).into()))
        );
    }

    #[test]
    fn empty_event_tags_errors() {
        let parser = CParser::default();
        let input = r#"MODALITY_PROBE_RECORD(probe, EVENT_A, MODALITY_TAGS(), "desc");"#;
        let tokens = parser.parse_event_md(input);
        assert_eq!(tokens, Err(Error::EmptyTags((38, 1, 39).into())));
        let input = r#"MODALITY_PROBE_RECORD(probe, EVENT_A, MODALITY_TAGS());"#;
        let tokens = parser.parse_event_md(input);
        assert_eq!(tokens, Err(Error::EmptyTags((38, 1, 39).into())));
        let input = r#"MODALITY_PROBE_RECORD_W_U32(probe, EVENT_A, 123, "desc", MODALITY_TAGS());"#;
        let tokens = parser.parse_event_md(input);
        assert_eq!(tokens, Err(Error::EmptyTags((57, 1, 58).into())));
    }

    #[test]
    fn severity_clamps() {
        let input = Span::new_extra("MODALITY_SEVERITY(0)", None);
        let (_, output) = modality_severity_as_tag(input).unwrap();
        assert_eq!(output, "SEVERITY_1".to_string());
        let input = Span::new_extra("MODALITY_SEVERITY(11)", None);
        let (_, output) = modality_severity_as_tag(input).unwrap();
        assert_eq!(output, "SEVERITY_10".to_string());
        let input = Span::new_extra("MODALITY_SEVERITY(5)", None);
        let (_, output) = modality_severity_as_tag(input).unwrap();
        assert_eq!(output, "SEVERITY_5".to_string());
    }

    #[test]
    fn empty_severity_errors() {
        let parser = CParser::default();
        let input = r#"MODALITY_PROBE_FAILURE(probe, EVENT_A, MODALITY_SEVERITY());"#;
        let tokens = parser.parse_event_md(input);
        assert_eq!(tokens, Err(Error::EmptySeverity((39, 1, 40).into())));
    }
}
